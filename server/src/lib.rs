mod world;

use ::world::TimeOfDay;
use anyhow::Error;
use block::{BlockId, REACH_DISTANCE, TexturePack};
use chunk::{ChunkMap, raycasting::raycast};
use ecs::{
    BoxCollider, Entity, EntityModel, EntityOrientation, EntityPosition, InteractionIntent,
    MovementIntent, SimulatedEntityBundle, eye_position, movement::MoveBundle,
};
use model::ModelDefinition;
use protocol::{
    CHANNEL_CHUNKS, CHANNEL_ENTITIES, ClientMessage, NetworkId, PROTOCOL_ID, Packet,
    ParticleEmitter, RENDER_DISTANCE, RENDER_DISTANCE_SQ, ServerMessage,
};
use renet::{RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};
use spatial::vectors::{Vec2iChunk, Vec3fGlobal, Vec3iGlobal};
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, UdpSocket},
    time::{Duration, SystemTime},
};

pub use world::{DefaultWorldGenerator, WorldGeneration, WorldGenerator, WorldGeneratorType};

use crate::world::GameWorld;

pub struct GameServer {
    server: RenetServer,
    transport: NetcodeServerTransport,

    world: GameWorld,
    chunks: ChunkMap,
    chunk_sweep_timer: Duration,

    texture_pack: TexturePack,

    entities: HashMap<NetworkId, Entity>,
    entities_inverted: HashMap<Entity, NetworkId>,

    client_states: ClientStates,
    stacks: SharedStacks,

    time_of_day: TimeOfDay,
    time_update_timer: Duration,
    emitter_seed_counter: u64,
}

#[derive(Default)]
struct SharedStacks {
    movement: HashMap<Entity, MoveBundle>,
}

const CHUNK_SWEEP_INTERVAL: Duration = Duration::from_secs(2);
const TIME_UPDATE_INTERVAL: Duration = Duration::from_millis(100); // Update time every 100ms
const MAX_CHUNKS_PER_TICK_PER_CLIENT: usize = 12;

#[derive(Default)]
struct ClientStates {
    sent_chunks: HashMap<NetworkId, HashSet<Vec2iChunk>>,
    player_positions: HashMap<NetworkId, Vec2iChunk>,
}

impl GameServer {
    pub fn new(
        bind_addr: SocketAddr,
        public_addr: SocketAddr,
        max_clients: usize,
        generator: WorldGeneration,
    ) -> Result<Self, Error> {
        let socket = UdpSocket::bind(bind_addr)?;

        let server_config = ServerConfig {
            current_time: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?,
            max_clients,
            protocol_id: PROTOCOL_ID,
            public_addresses: vec![public_addr],
            authentication: ServerAuthentication::Unsecure,
        };

        let transport = NetcodeServerTransport::new(server_config, socket)?;
        let server = RenetServer::new(Default::default()); //connection_config());
        Ok(Self {
            server,
            transport,
            world: GameWorld::new(generator),
            chunks: ChunkMap::new_persistent("world/chunks")?,
            texture_pack: TexturePack::load(),
            client_states: Default::default(),
            entities: Default::default(),
            entities_inverted: Default::default(),
            stacks: Default::default(),
            chunk_sweep_timer: Duration::ZERO,
            time_of_day: TimeOfDay::new(0.3), // Start at early morning
            time_update_timer: Duration::ZERO,
            emitter_seed_counter: 0,
        })
    }

    pub fn update(&mut self, dt: Duration) -> Result<(), Error> {
        self.server.update(dt);

        if let Err(e) = self.transport.update(dt, &mut self.server) {
            anyhow::bail!("Transport update failed: {e}");
        }

        self.process_events()?;
        self.receive_messages()?;

        self.time_update_timer += dt;
        if self.time_update_timer >= TIME_UPDATE_INTERVAL {
            self.time_update_timer = Duration::ZERO;
            // TODO: 20 minutes (1200 seconds)
            self.time_of_day
                .advance(TIME_UPDATE_INTERVAL.as_secs_f32() / 60.0);

            let msg = ServerMessage::ServerTime(self.time_of_day);
            if let Ok(payload) = msg.encode() {
                for net_id in self.entities.keys() {
                    self.server
                        .send_message(**net_id, CHANNEL_ENTITIES, payload.clone());
                }
            }
        }

        self.stream_chunks_for_players()?;

        ecs::movement::apply_intent_all(
            self.world.world_mut(),
            &mut self.stacks.movement,
            &self.chunks,
            &self.texture_pack,
            dt,
        );

        if let Err(err) = self.chunks.persist_dirty(24) {
            tracing::warn!("Failed to enqueue dirty chunk persistence: {err:#}");
        }

        self.chunks.poll_persistence();

        self.emit_entities();

        let swept = self.sweep_chunks(dt);
        if swept > 0 {
            tracing::debug!(
                "Chunk cleanup: {swept} ({} currently loaded)",
                self.chunks.chunk_count(),
            );
        }

        self.transport.send_packets(&mut self.server);

        Ok(())
    }

    fn stream_chunks_for_players(&mut self) -> Result<(), Error> {
        for (client_id, chunk) in self.client_states.player_positions.clone() {
            self.send_nearby_chunks(
                client_id,
                chunk,
                RENDER_DISTANCE,
                MAX_CHUNKS_PER_TICK_PER_CLIENT,
            )?;
        }

        Ok(())
    }

    fn set_block_and_sync(
        &mut self,
        world_position: Vec3fGlobal,
        block_id: BlockId,
    ) -> Result<Option<BlockId>, Error> {
        let block_position = Vec3iGlobal::from([
            world_position.x().floor() as i32,
            world_position.y().floor() as i32,
            world_position.z().floor() as i32,
        ]);
        let chunk_coordinate = Vec2iChunk::from(world_position);

        let Some(before) = self.world.get_block(&self.chunks, world_position) else {
            return Ok(None);
        };

        if before == block_id {
            return Ok(None);
        }

        if self
            .world
            .set_block(&mut self.chunks, world_position, block_id)
            .is_none()
        {
            return Ok(None);
        }

        let block_edit_msg = ServerMessage::BlockEdit {
            position: block_position,
            before,
            after: block_id,
        }
        .encode()?;

        let recipients: Vec<_> = self
            .client_states
            .player_positions
            .keys()
            .filter_map(|client_id| {
                self.client_states
                    .sent_chunks
                    .get(client_id)
                    .and_then(|sent_chunks| {
                        if sent_chunks.contains(&chunk_coordinate) {
                            Some(*client_id)
                        } else {
                            None
                        }
                    })
            })
            .collect();

        for client_id in recipients {
            if !self
                .server
                .can_send_message(*client_id, CHANNEL_CHUNKS, block_edit_msg.len())
            {
                tracing::warn!(
                    "Backpressure while syncing edited chunk {chunk_coordinate:?} to {client_id:?}"
                );
                continue;
            }

            self.server
                .send_message(*client_id, CHANNEL_CHUNKS, block_edit_msg.clone());

            self.client_states
                .sent_chunks
                .entry(client_id)
                .or_default()
                .insert(chunk_coordinate);
        }

        Ok(Some(before))
    }

    // TODO
    fn block_particle_color(block: BlockId) -> [u8; 4] {
        match block.0 {
            1 => [133, 107, 74, 220],
            2 => [126, 160, 101, 220],
            3 => [130, 130, 130, 220],
            4 => [191, 173, 140, 220],
            _ => [180, 180, 180, 220],
        }
    }

    fn emit_block_break_particles(
        &mut self,
        block_position: Vec3iGlobal,
        normal: Vec3iGlobal,
        broken_block: BlockId,
    ) -> Result<(), Error> {
        let chunk_coordinate = Vec2iChunk::from(block_position);

        self.emitter_seed_counter = self.emitter_seed_counter.wrapping_add(1);

        let emitter = ParticleEmitter {
            origin: Vec3fGlobal::new(
                block_position[0] as f32 + 0.5 + normal[0] as f32 * 0.08,
                block_position[1] as f32 + 0.5 + normal[1] as f32 * 0.08,
                block_position[2] as f32 + 0.5 + normal[2] as f32 * 0.08,
            ),
            normal: Vec3fGlobal::new(normal[0] as f32, normal[1] as f32, normal[2] as f32),
            seed: self.emitter_seed_counter
                ^ ((block_position[0] as i64 as u64) << 32)
                ^ ((block_position[1] as i64 as u64) << 16)
                ^ (block_position[2] as i64 as u64),
            emission_rate: 220,
            emission_duration_ms: 95,
            particle_lifetime_ms: 480,
            max_particles: 52,
            initial_speed: 2.8,
            spread: 0.85,
            gravity: -10.8,
            size_start: 2.8,
            size_end: 0.4,
            color: Self::block_particle_color(broken_block),
        };

        let msg = ServerMessage::ParticleSpawn { emitter }.encode()?;

        // TODO: client can filter particles
        for (client_id, _) in self
            .client_states
            .player_positions
            .iter()
            .filter(|(_, chunk)| (**chunk - chunk_coordinate).length_sq() <= RENDER_DISTANCE_SQ)
        {
            self.server
                .send_message(**client_id, CHANNEL_ENTITIES, msg.clone());
        }

        Ok(())
    }

    fn sweep_chunks(&mut self, dt: Duration) -> usize {
        self.chunk_sweep_timer += dt;
        if self.chunk_sweep_timer >= CHUNK_SWEEP_INTERVAL {
            self.chunk_sweep_timer = Duration::ZERO;

            let positions: Vec<_> = self
                .client_states
                .player_positions
                .values()
                .copied()
                .collect();

            if !positions.is_empty() {
                self.world
                    .unload_distant_chunks(&mut self.chunks, &positions, 2 * RENDER_DISTANCE)
            } else {
                0
            }
        } else {
            0
        }
    }

    fn emit_entities(&mut self) {
        for (moved, move_bundle) in self.stacks.movement.drain() {
            let Some(&entity_id) = self.entities_inverted.get(&moved) else {
                tracing::warn!("Entity moved but has no network id");
                continue;
            };

            let origin_chunk = Vec2iChunk::from(move_bundle.position());

            self.client_states
                .player_positions
                .insert(entity_id, origin_chunk);

            let move_msg = match (move_bundle.velocity(), move_bundle.collision()) {
                (Some(velocity), Some(collision_status)) => {
                    let move_msg = ServerMessage::EntityMove {
                        entity_id,
                        position: EntityPosition(move_bundle.position()),
                        velocity,
                        collision_status,
                    };

                    move_msg.encode().ok()
                }
                _ => None,
            };

            let look_msg = move_bundle.orientation().map(|orientation| {
                let look_msg = ServerMessage::EntityLook {
                    entity_id,
                    orientation,
                };

                look_msg.encode().unwrap()
            });

            for (client_id, _) in self
                .client_states
                .player_positions
                .iter()
                .filter(|(_, chunk)| (**chunk - origin_chunk).length_sq() <= RENDER_DISTANCE_SQ)
            {
                if let Some(entity_move) = move_msg.clone() {
                    self.server
                        .send_message(**client_id, CHANNEL_ENTITIES, entity_move);
                }
                if let Some(entity_look) = look_msg.clone() {
                    self.server
                        .send_message(**client_id, CHANNEL_ENTITIES, entity_look);
                }
            }
        }
    }

    fn process_events(&mut self) -> Result<(), Error> {
        while let Some(event) = self.server.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    let bundle = SimulatedEntityBundle::new(
                        EntityPosition([0.0, 100.0, 0.0].into()),
                        Default::default(),
                        Default::default(),
                        Default::default(),
                        Default::default(),
                        BoxCollider(spatial::aabb::BoxCollider::for_model(
                            ModelDefinition::Humanoid,
                        )),
                        EntityModel::for_model(ModelDefinition::Humanoid),
                        Default::default(),
                    );
                    let client_id = NetworkId(client_id);

                    let (entity_mut, position) = self.world.spawn(
                        &mut self.server,
                        self.entities
                            .keys()
                            .copied()
                            .chain(std::iter::once(client_id)),
                        client_id,
                        bundle,
                    )?;
                    let entity = entity_mut.id();

                    self.entities.insert(client_id, entity);
                    self.entities_inverted.insert(entity, client_id);

                    self.client_states
                        .sent_chunks
                        .insert(client_id, Default::default());
                    self.client_states
                        .player_positions
                        .insert(client_id, Vec2iChunk::from(position.0));

                    self.sync_existing_entities(client_id, position)?;
                }
                ServerEvent::ClientDisconnected { client_id, .. } => {
                    let client_id = NetworkId(client_id);
                    if let Some(entity) = self.entities.remove(&client_id) {
                        self.world.despawn(
                            &mut self.server,
                            self.entities.keys().copied(),
                            client_id,
                            entity,
                        );
                        self.entities_inverted.remove(&entity);
                    }
                    self.client_states.sent_chunks.remove(&client_id);
                    self.client_states.player_positions.remove(&client_id);
                }
            }
        }

        Ok(())
    }

    fn receive_messages(&mut self) -> Result<(), Error> {
        for client_id in self.server.clients_id() {
            let network_id = NetworkId(client_id);
            let Some(&entity) = self.entities.get(&network_id) else {
                continue;
            };

            // TODO: on new channel?
            while let Some(msg) = self.server.receive_message(client_id, CHANNEL_ENTITIES) {
                let msg = ClientMessage::decode(&msg)?;

                match msg {
                    ClientMessage::EntityRemodel {
                        model,
                        bounding_box,
                    } => {
                        let mut query = self.world.world_mut().entity_mut(entity);
                        let Ok((mut entity_model, mut entity_collider)) =
                            query.get_components_mut::<(&mut EntityModel, &mut BoxCollider)>()
                        else {
                            continue;
                        };

                        *entity_model = model;
                        *entity_collider = bounding_box;

                        let Some(&origin_chunk) =
                            self.client_states.player_positions.get(&network_id)
                        else {
                            continue;
                        };

                        let msg = ServerMessage::EntityRemodel {
                            entity_id: network_id,
                            model,
                            bounding_box,
                        }
                        .encode()?;

                        self.reemit(msg, network_id, origin_chunk);
                    }
                    ClientMessage::BlockInteract {
                        intent,
                        targeted_block,
                    } => {
                        let mut query = self.world.world_mut().entity_mut(entity);
                        let Ok((mut current_intent, position, orientation, model)) = query
                            .get_components_mut::<(
                                &mut InteractionIntent,
                                &EntityPosition,
                                &EntityOrientation,
                                &EntityModel,
                            )>()
                        else {
                            continue;
                        };

                        *current_intent = intent;

                        if !intent.attack {
                            continue;
                        }

                        let Some((target_position, target_normal)) = targeted_block else {
                            continue;
                        };

                        tracing::debug!(
                            "Server saw block at {target_position:?} with normal {target_normal:?}, player position = {:?}, orientation = {:?}, eye height = {:?}",
                            position.0,
                            orientation.0,
                            model.eye_height,
                        );
                        let ray_origin = eye_position(position.0, model.eye_height);

                        let ray = raycast(
                            ray_origin,
                            orientation.0,
                            REACH_DISTANCE,
                            &self.chunks,
                            &self.texture_pack,
                        );

                        if let Some((block, normal)) = ray {
                            if block.position() != target_position || normal != target_normal {
                                tracing::debug!(
                                    "Rejected block break due to target mismatch: client={target_position:?}/{target_normal:?}, server={:?}/{normal:?}",
                                    block.position()
                                );
                                continue;
                            }

                            if let Some(before) =
                                self.set_block_and_sync(block.position().into(), BlockId::AIR)?
                            {
                                self.emit_block_break_particles(block.position(), normal, before)?;
                            }
                        }
                    }
                    ClientMessage::EntityMove(intent) => {
                        let Some(mut current_intent) =
                            self.world.world_mut().get_mut::<MovementIntent>(entity)
                        else {
                            continue;
                        };
                        *current_intent = intent;
                    }
                    ClientMessage::EntityLook(orientation) => {
                        let Some(mut current_orientation) =
                            self.world.world_mut().get_mut::<EntityOrientation>(entity)
                        else {
                            continue;
                        };

                        if *current_orientation == orientation {
                            continue;
                        }

                        *current_orientation = orientation;

                        let Some(&origin_chunk) =
                            self.client_states.player_positions.get(&network_id)
                        else {
                            continue;
                        };

                        let msg = ServerMessage::EntityLook {
                            entity_id: network_id,
                            orientation,
                        }
                        .encode()?;

                        self.reemit(msg, network_id, origin_chunk);
                    }
                }
            }
        }

        Ok(())
    }

    fn reemit(&mut self, msg: Vec<u8>, origin_id: NetworkId, origin_chunk: Vec2iChunk) {
        for (observer_id, _) in self
            .client_states
            .player_positions
            .iter()
            // don't replay the look message back to the client that sent it
            .filter(|(observer_id, _)| **observer_id != origin_id)
            .filter(|(_, chunk)| (**chunk - origin_chunk).length_sq() <= RENDER_DISTANCE_SQ)
        {
            self.server
                .send_message(**observer_id, CHANNEL_ENTITIES, msg.clone());
        }
    }

    /// Tell a newly connected client about all existing entities within render distance
    fn sync_existing_entities(
        &mut self,
        client_id: NetworkId,
        position: EntityPosition,
    ) -> Result<(), Error> {
        let msg = ServerMessage::ClientSpawned(client_id).encode()?;
        self.server.send_message(*client_id, CHANNEL_ENTITIES, msg);

        let observer_chunk = Vec2iChunk::from(position.0);

        for (other_client_id, &position, &bounding_box, &model) in
            self.entities
                .iter()
                .filter_map(|(other_client_id, other_entity)| {
                    if *other_client_id == client_id {
                        return None;
                    }

                    let (position, bounding_box, model) = self
                        .world
                        .world()
                        .entity(*other_entity)
                        .get_components::<(&EntityPosition, &BoxCollider, &EntityModel)>()
                        .ok()?;
                    let other_chunk = Vec2iChunk::from(position.0);

                    if (other_chunk - observer_chunk).length_sq() > RENDER_DISTANCE_SQ {
                        return None;
                    }

                    Some((*other_client_id, position, bounding_box, model))
                })
        {
            let msg = ServerMessage::EntitySpawn {
                entity_id: other_client_id,
                position,
                bounding_box,
                model,
            }
            .encode()?;

            self.server.send_message(*client_id, CHANNEL_ENTITIES, msg);
        }

        Ok(())
    }

    fn send_nearby_chunks(
        &mut self,
        client_id: NetworkId,
        coordinate: Vec2iChunk,
        max_distance: i32,
        max_chunks: usize,
    ) -> Result<(), Error> {
        let mut sent_this_call = 0;

        let center_cx = coordinate.x();
        let center_cz = coordinate.z();

        let sent = self.client_states.sent_chunks.entry(client_id).or_default();
        sent.retain(|coord| (*coord - coordinate).length_sq() <= max_distance * max_distance);

        for cx in (center_cx - max_distance / 2)..=(center_cx + max_distance / 2) {
            for cz in (center_cz - max_distance / 2)..=(center_cz + max_distance / 2) {
                let coordinate = Vec2iChunk::from([cx, cz]);

                if sent.contains(&coordinate) {
                    continue;
                }

                if sent_this_call >= max_chunks {
                    tracing::debug!("Chunk transmission saturated at {sent_this_call}");
                    return Ok(());
                }

                let Some(chunk) = self.world.generate(&mut self.chunks, coordinate)? else {
                    continue;
                };
                let msg = ServerMessage::ChunkData(chunk).encode()?;

                if !self
                    .server
                    .can_send_message(*client_id, CHANNEL_CHUNKS, msg.len())
                {
                    tracing::warn!(
                        "Backpressure for {client_id:?}: available={} wanted={}",
                        self.server
                            .channel_available_memory(*client_id, CHANNEL_CHUNKS),
                        msg.len(),
                    );
                    return Ok(());
                }

                self.server.send_message(*client_id, CHANNEL_CHUNKS, msg);
                sent.insert(coordinate);
                sent_this_call += 1;
            }
        }

        Ok(())
    }
}
