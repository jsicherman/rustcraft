pub mod camera;
mod event_handler;
mod key_handler;
pub mod settings;
pub mod world;

use crate::{
    camera::Camera,
    settings::AppConfig,
    world::{ChunkCache, ClientRenderable, EntityCache},
};
use block::{BlockId, BlockRegistry};
use chunk::Chunk;
use ecs::{BoxCollider, Entity, EntityOrientation, EntityPosition, MovementIntent, World};
use protocol::{NetworkId, PROTOCOL_ID, RENDER_DISTANCE_SQ};
use render::{DebugOverlayData, Renderer};
use renet::RenetClient;
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};
use spatial::vectors::{Global, IntoSpace, Vec2iChunk, Vec3fGlobal};
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Instant, SystemTime},
};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
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
    local_player_network_id: Option<NetworkId>,

    network_to_local: HashMap<NetworkId, Entity>,

    pressed_keys: HashSet<KeyCode>,
    last_sent_intent: Option<MovementIntent>,
    last_sent_orientation: Option<EntityOrientation>,
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
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
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
        let world = World::new();

        self.state = Some(AppState {
            window,
            renderer,
            camera,
            client,
            transport,
            local_player_network_id: None,
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

        // state.window.set_cursor_visible(false);

        let event_consumed = state.renderer.handle_window_event(&state.window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorEntered { .. } if !event_consumed => {
                state
                    .window
                    .set_cursor_grab(CursorGrabMode::Confined)
                    .unwrap();
            }
            WindowEvent::CursorLeft { .. } if !event_consumed => {
                let _ = state.window.set_cursor_grab(CursorGrabMode::None);
            }
            WindowEvent::CursorMoved { mut position, .. } if !event_consumed => {
                let size = state.window.inner_size();

                let width = size.width as f64;

                let mut wrapped = false;
                if position.x < 1.0 {
                    wrapped = true;
                    position.x = width - 4.0;
                } else if position.x > width - 3.0 {
                    wrapped = true;
                    position.x = 2.0;
                }

                tracing::debug!("cursor moved to {:?}", position);

                if wrapped {
                    let _ = state.window.set_cursor_position(position);
                    state.camera.reset_cursor_position(position.x, position.y);
                } else {
                    state.camera.handle_cursor_moved(position.x, position.y);
                }
            }
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
    fn request_entities(&mut self, my_position: Vec3fGlobal) {
        let my_chunk = Vec2iChunk::from(my_position);

        for (id, client_entity) in self.loaded_entities.entities.iter_mut() {
            // Don't render the local player
            if Some(*id) == self.local_player_network_id {
                continue;
            }

            if client_entity.is_queued()
                || (client_entity.has_meshes() && !client_entity.is_dirty())
            {
                continue;
            }

            let Some(local) = self.network_to_local.get(id) else {
                continue;
            };

            let Ok((their_position, their_collider)) = self
                .world
                .entity(*local)
                .get_components::<(&EntityPosition, &BoxCollider)>()
            else {
                continue;
            };

            let their_chunk = Vec2iChunk::from(their_position.0);
            if (their_chunk - my_chunk).length_sq() > RENDER_DISTANCE_SQ {
                continue;
            }

            client_entity.queue_mesh();

            let voxels = vec![BlockId::Missing as u32; their_collider.0.volume()];
            let corner_position = their_position.0
                - Vec3fGlobal::new(
                    their_collider.0.half_width(),
                    0.0,
                    their_collider.0.half_width(),
                );
            self.renderer.entity_builder.enqueue(
                id.0,
                voxels,
                [
                    (their_collider.0.half_width() * 2.0) as usize,
                    their_collider.0.height() as usize,
                    (their_collider.0.half_width() * 2.0) as usize,
                ],
                corner_position.into(),
            );
        }

        for result in self.renderer.entity_builder.collect_results() {
            let network_id = NetworkId(result.key);

            let Some(client_entity) = self.loaded_entities.entities.get_mut(&network_id) else {
                continue;
            };

            let Some(local) = self.network_to_local.get(&network_id) else {
                continue;
            };

            let Ok((their_position, their_orientation)) = self
                .world
                .entity(*local)
                .get_components::<(&EntityPosition, &EntityOrientation)>()
            else {
                continue;
            };

            let their_chunk = Vec2iChunk::from(their_position.0);
            if (their_chunk - my_chunk).length_sq() > RENDER_DISTANCE_SQ {
                client_entity.unqueue_mesh();
                continue;
            }

            let gpu_mesh = self.renderer.upload_cpu_mesh_rotated(
                result.mesh,
                [
                    their_position.0.x(),
                    their_position.0.y(),
                    their_position.0.z(),
                ],
                their_orientation.0.yaw(),
                their_orientation.0.pitch(),
            );
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
}
