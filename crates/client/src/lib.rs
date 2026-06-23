pub mod camera;
pub mod world;

use crate::{
    camera::Camera,
    world::{ChunkCache, ClientChunk, ClientRenderable, EntityCache},
};
use block::{BlockId, BlockRegistry};
use chunk::Chunk;
use ecs::{
    BoxCollider, CollisionStatus, Entity, EntityPosition, EntityVelocity, LocalPlayer,
    MovementIntent, Orientation, SimulatedEntityBundle, World,
};
use protocol::{
    CHANNEL_CHUNKS, CHANNEL_ENTITIES, ClientMessage, NetworkId, PROTOCOL_ID, Packet,
    RENDER_DISTANCE_SQ, ServerMessage,
};
use render::{DebugOverlayData, Renderer};
use renet::RenetClient;
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};
use serde::Deserialize;
use spatial::vectors::{Global, IntoSpace, Vec2iChunk, Vec3fGlobal};
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, UdpSocket},
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

#[derive(Default)]
pub struct App {
    state: Option<AppState>,
    config: AppConfig,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }
}

#[derive(Default, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub host: HostConfig,
}

#[derive(Deserialize)]
pub struct ServerConfig {
    pub address: String,
    pub port: u16,
}

#[derive(Deserialize)]
pub struct HostConfig {
    pub tps: u64,
    pub max_clients: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            tps: 60,
            max_clients: 64,
        }
    }
}

pub fn load_config(path: Option<&Path>) -> AppConfig {
    let config_str = std::fs::read_to_string(path.unwrap_or(Path::new("config.toml")));
    config_str
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub struct AppState {
    window: Arc<Window>,
    renderer: Renderer,

    client: RenetClient,
    transport: NetcodeClientTransport,

    last_update: Instant,

    block_registry: BlockRegistry,

    loaded_chunks: ChunkCache,
    loaded_entities: EntityCache,

    camera: Camera,
    world: World,
    player_entity: Entity,

    network_to_local: HashMap<NetworkId, Entity>,

    pressed_keys: HashSet<KeyCode>,
    last_sent_intent: Option<MovementIntent>,
    last_sent_orientation: Option<Orientation>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_inner_size(PhysicalSize::new(1280u32, 720u32)),
                )
                .unwrap(),
        );

        let block_registry = BlockRegistry::load();
        let renderer = pollster::block_on(render::init(Arc::clone(&window), &block_registry));

        let client = RenetClient::new(Default::default());
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let transport = NetcodeClientTransport::new(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap(),
            ClientAuthentication::Unsecure {
                protocol_id: PROTOCOL_ID,
                client_id: rand::random(),
                server_addr: format!("{}:{}", self.config.server.address, self.config.server.port)
                    .parse::<SocketAddr>()
                    .unwrap(),
                user_data: None,
            },
            socket,
        )
        .unwrap();

        let camera = Camera::new();
        let mut world = World::new();

        let player_entity = world
            .spawn((SimulatedEntityBundle::default(), LocalPlayer))
            .id();

        self.state = Some(AppState {
            window,
            renderer,
            camera,
            client,
            transport,
            player_entity,
            world,
            block_registry,
            loaded_chunks: Default::default(),
            loaded_entities: Default::default(),
            network_to_local: Default::default(),
            pressed_keys: Default::default(),
            last_sent_intent: None,
            last_sent_orientation: None,
            last_update: Instant::now(),
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };

        let event_consumed = state.renderer.handle_window_event(&state.window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state: key_state,
                        ..
                    },
                ..
            } if !event_consumed => match key_state {
                ElementState::Pressed => {
                    state.pressed_keys.insert(key);

                    if key == KeyCode::Escape {
                        event_loop.exit();
                    }
                }
                ElementState::Released => {
                    state.pressed_keys.remove(&key);
                }
            },
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now - state.last_update;
                state.last_update = now;

                let (position, orientation) = state.process_inputs(dt);
                let chunk_position = Vec2iChunk::from(position);

                state.receive_chunks(chunk_position);
                state.request_chunks(chunk_position);

                state.receive_entities(chunk_position);
                state.request_entities(position);

                let mut active_meshes = Vec::new();

                active_meshes.extend(
                    state
                        .loaded_chunks
                        .chunks
                        .values()
                        .filter_map(ClientRenderable::meshes),
                );

                let total_indices: u32 = active_meshes.iter().map(|mesh| mesh.index_count()).sum();

                active_meshes.extend(
                    state
                        .loaded_entities
                        .entities
                        .values()
                        .filter_map(ClientRenderable::meshes),
                );

                let total_indices_with_entities: u32 =
                    active_meshes.iter().map(|mesh| mesh.index_count()).sum();
                let total_entity_meshes = total_indices_with_entities - total_indices;

                state.transport.send_packets(&mut state.client).ok();

                let size = state.window.inner_size();

                state
                    .camera
                    .set_aspect(size.width as f32 / size.height as f32);

                let vp = state.camera.view_projection(position, orientation);

                let debug_overlay = DebugOverlayData {
                    player_pos: position.into(),
                    yaw_radians: orientation.yaw(),
                    pitch_radians: orientation.pitch(),
                    index_count: total_indices,
                    entity_index_count: total_entity_meshes,
                    frame_time_ms: dt.as_millis(),
                };

                state.renderer.render(
                    &state.window,
                    &active_meshes,
                    vp.map(std::convert::Into::into),
                    &debug_overlay,
                );

                state.window.request_redraw();
            }
            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
            }
            _ => {}
        }
    }
}

impl AppState {
    fn process_inputs(
        &mut self,
        dt: Duration,
    ) -> (
        spatial::vectors::Vec3fGlobal,
        spatial::orientation::Orientation,
    ) {
        let axis = |positive: KeyCode, negative: KeyCode| -> f32 {
            (self.pressed_keys.contains(&positive) as u8 as f32)
                - (self.pressed_keys.contains(&negative) as u8 as f32)
        };

        let forward = axis(KeyCode::KeyW, KeyCode::KeyS);
        let right = axis(KeyCode::KeyA, KeyCode::KeyD);
        let up = axis(KeyCode::Space, KeyCode::ShiftLeft);

        let mut entity = self.world.entity_mut(self.player_entity);

        let (
            mut position,
            mut velocity,
            mut orientation,
            mut intent,
            collider,
            mut collision_status,
        ) = entity
            .get_components_mut::<(
                &mut EntityPosition,
                &mut EntityVelocity,
                &mut Orientation,
                &mut MovementIntent,
                &BoxCollider,
                &mut CollisionStatus,
            )>()
            .unwrap();

        let look_speed = 1.8 * dt.as_secs_f32();
        orientation
            .0
            .yaw_pitch(
                axis(KeyCode::ArrowLeft, KeyCode::ArrowRight) * look_speed,
                axis(KeyCode::ArrowUp, KeyCode::ArrowDown) * look_speed,
            )
            .clamp(.., -1.5..1.5);

        self.client.update(dt);
        self.transport.update(dt, &mut self.client).ok();

        let new_intent = MovementIntent::new(forward, right, up > 0.0, false, false);
        if new_intent != *intent {
            *intent = new_intent;
        }

        let should_sync_intent = self.last_sent_intent != Some(*intent);
        let should_sync_orientation = self.last_sent_orientation != Some(*orientation);

        if self.client.is_connected() {
            if should_sync_intent {
                let msg = ClientMessage::Move(*intent).encode().unwrap();
                self.client.send_message(CHANNEL_CHUNKS, msg);
            }

            if should_sync_orientation {
                let msg = ClientMessage::Look(*orientation).encode().unwrap();
                self.client.send_message(CHANNEL_CHUNKS, msg);
            }
        }

        if should_sync_intent {
            self.last_sent_intent = Some(*intent);
        }
        if should_sync_orientation {
            self.last_sent_orientation = Some(*orientation);
        }

        let new_velocity = ecs::movement::apply_gravity(velocity.0, &intent, *collision_status, dt);

        let (new_position, new_velocity) =
            ecs::movement::apply_intent(position.0, orientation.0, &intent, new_velocity, dt);

        let (final_position, final_velocity, new_status) = ecs::movement::apply_collision_aabb(
            new_position,
            *collider,
            *collision_status,
            new_velocity,
            &self.loaded_chunks,
            &self.block_registry,
            dt,
        );

        *position = EntityPosition(final_position);
        *velocity = EntityVelocity(final_velocity);
        *collision_status = new_status;

        (position.0, orientation.0)
    }

    fn request_entities(&mut self, my_position: Vec3fGlobal) {
        let my_chunk = Vec2iChunk::from(my_position);

        for (id, client_entity) in self.loaded_entities.entities.iter_mut() {
            if client_entity.has_meshes() || client_entity.is_queued() {
                continue;
            }

            let Some(local) = self.network_to_local.get(id) else {
                continue;
            };

            let Some(their_position) = self.world.get::<EntityPosition>(*local) else {
                continue;
            };

            let their_chunk = Vec2iChunk::from(their_position.0);
            if (their_chunk - my_chunk).length_sq() > RENDER_DISTANCE_SQ {
                continue;
            }

            client_entity.queue_mesh();

            let voxels = vec![BlockId::Missing as u32, BlockId::Missing as u32];
            self.renderer
                .entity_builder
                .enqueue(id.0, voxels, [1, 2, 1], their_position.0.into());
        }

        for result in self.renderer.entity_builder.collect_results() {
            let network_id = NetworkId(result.key);

            tracing::debug!("received mesh for {network_id:?}");

            let Some(client_entity) = self.loaded_entities.entities.get_mut(&network_id) else {
                continue;
            };

            tracing::debug!("processing mesh for {network_id:?}");

            let Some(local) = self.network_to_local.get(&network_id) else {
                continue;
            };

            tracing::debug!("found local entity for {network_id:?}");

            let Some(their_position) = self.world.get::<EntityPosition>(*local) else {
                continue;
            };

            tracing::debug!("found position for {network_id:?}: {:?}", their_position.0);

            let their_chunk = Vec2iChunk::from(their_position.0);
            if (their_chunk - my_chunk).length_sq() > RENDER_DISTANCE_SQ {
                client_entity.unqueue_mesh();
                continue;
            }

            tracing::debug!("uploading mesh for {network_id:?}");

            let gpu_mesh = self.renderer.upload_cpu_mesh(result.mesh);
            client_entity.provide_mesh(gpu_mesh);
        }
    }

    fn request_chunks(&mut self, position: Vec2iChunk) {
        for (coordinate, client_chunk) in self.loaded_chunks.chunks.iter_mut() {
            if client_chunk.has_meshes() || client_chunk.is_queued() {
                continue;
            }

            if (*coordinate - position).length_sq() > RENDER_DISTANCE_SQ {
                continue;
            }

            client_chunk.queue_mesh();

            let voxels: Vec<_> = client_chunk.iter().map(|b| b as u32).collect();
            let global = IntoSpace::<Global>::into_space(*coordinate);
            self.renderer.chunk_builder.enqueue(
                (coordinate.x(), coordinate.z()),
                voxels,
                Chunk::CHUNK_COLUMN,
                [global.x() as f32, 0.0, global.z() as f32],
            );
        }

        for result in self.renderer.chunk_builder.collect_results() {
            let coordinate = Vec2iChunk::from(result.key);

            let Some(client_chunk) = self.loaded_chunks.chunks.get_mut(&coordinate) else {
                continue;
            };

            if (coordinate - position).length_sq() > RENDER_DISTANCE_SQ {
                client_chunk.unqueue_mesh();
                continue;
            }

            let gpu_mesh = self.renderer.upload_cpu_mesh(result.mesh);
            client_chunk.provide_mesh(gpu_mesh);
        }
    }

    fn receive_chunks(&mut self, chunk_position: Vec2iChunk) {
        self.loaded_chunks
            .chunks
            .retain(|coord, _| (*coord - chunk_position).length_sq() <= RENDER_DISTANCE_SQ);

        while let Some(msg) = self.client.receive_message(CHANNEL_CHUNKS) {
            let msg = ServerMessage::decode(&msg).unwrap();

            match msg {
                ServerMessage::ChunkData(chunk) => {
                    let coordinate = chunk.coordinate();

                    if (coordinate - chunk_position).length_sq() > RENDER_DISTANCE_SQ {
                        continue;
                    }

                    self.loaded_chunks
                        .chunks
                        .insert(coordinate, ClientChunk::new(*chunk));
                }
                ServerMessage::EntityMove { .. }
                | ServerMessage::EntityLook { .. }
                | ServerMessage::EntitySpawn { .. }
                | ServerMessage::EntityDespawn(_) => {
                    unreachable!()
                }
            }
        }
    }

    fn receive_entities(&mut self, _chunk_position: Vec2iChunk) {
        while let Some(msg) = self.client.receive_message(CHANNEL_ENTITIES) {
            let msg = ServerMessage::decode(&msg).unwrap();

            match msg {
                ServerMessage::ChunkData(_) => unreachable!(),
                ServerMessage::EntityMove {
                    entity_id,
                    position,
                    velocity,
                    collision_status,
                } => {
                    let Some(entity) = self.network_to_local.get(&entity_id) else {
                        continue;
                    };

                    let Ok(mut entity) = self.world.get_entity_mut(*entity) else {
                        continue;
                    };

                    let Ok((mut client_position, mut client_velocity, mut client_collision_status)) =
                        entity.get_components_mut::<(
                            &mut EntityPosition,
                            &mut EntityVelocity,
                            &mut CollisionStatus,
                        )>()
                    else {
                        continue;
                    };

                    /*tracing::debug!(
                        "dP: {:.3?} {}",
                        position.0 - client_position.0,
                        *client_collision_status == collision_status
                    );
                    tracing::debug!("dV: {:.3?}", velocity.0 - client_velocity.0);*/

                    let position_changed = *client_position != position;

                    *client_position = position;
                    *client_velocity = velocity;
                    *client_collision_status = collision_status;

                    // Entity meshes are baked with world-space offsets, so they must be
                    // rebuilt when the entity position changes.
                    // FIXME: check this
                    if position_changed
                        && let Some(client_entity) =
                            self.loaded_entities.entities.get_mut(&entity_id)
                    {
                        *client_entity = Default::default();
                    }
                }
                ServerMessage::EntityLook {
                    entity_id,
                    orientation,
                } => {
                    let Some(entity) = self.network_to_local.get(&entity_id) else {
                        continue;
                    };

                    let Ok(mut entity) = self.world.get_entity_mut(*entity) else {
                        continue;
                    };

                    if let Some(mut client_orientation) = entity.get_mut::<Orientation>() {
                        *client_orientation = orientation;
                    }
                }
                ServerMessage::EntitySpawn {
                    entity_id,
                    position,
                } => {
                    // FIXME: double spawn for client (here and reload)?
                    tracing::debug!("spawning entity {:?} at {:?}", entity_id, position);

                    self.network_to_local.insert(
                        entity_id,
                        self.world
                            .spawn(SimulatedEntityBundle {
                                position,
                                ..Default::default()
                            })
                            .id(),
                    );
                    self.loaded_entities
                        .entities
                        .insert(entity_id, Default::default());
                }
                ServerMessage::EntityDespawn(entity_id) => {
                    if let Some(entity) = self.network_to_local.remove(&entity_id) {
                        self.world.despawn(entity);
                    }
                }
            }
        }
    }
}
