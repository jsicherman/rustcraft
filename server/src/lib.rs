mod world;

use ::world::TimeOfDay;
use anyhow::Error;
use bevy_ecs::query::With;
use chunk::{ChunkMap, raycasting::raycast};
use ecs::ai::NpcController;
use ecs::{
    BoxCollider, Entity, EntityBundle, EntityModel, EntityOrientation, EntityPosition,
    InteractionIntent, MovementIntent, eye_position, movement::MoveBundle,
};
use ecs::{NpcBundle, Pathfinding};
use protocol::entity::{ClientMessage, EntityMessage, pathfinding_tick};
use protocol::particles::ParticleEmitter;
use protocol::world::WorldMessage;
use protocol::{ClientBound, NetworkId, PROTOCOL_ID, Packet, RENDER_DISTANCE, RENDER_DISTANCE_SQ};
use renet::{RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};
use resources::entity::ModelDefinition;
use resources::{ResourcePack, block::BlockId};
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

    resource_pack: ResourcePack,

    client_states: ClientStates,
    entity_states: EntityStates,

    stacks: SharedStacks,

    time_of_day: TimeOfDay,
    game_loop: GameLoop,

    entity_id_counter: u64,
    emitter_seed_counter: u64,
}

pub struct GameLoop {
    tick: u64,
    chunk_sweep_rate: u64,
    physics_tick_rate: u64,
    reconcile_position_rate: u64,
    ai_tick_rate: u64,
}

impl Default for GameLoop {
    fn default() -> Self {
        Self {
            tick: 0,
            chunk_sweep_rate: 2000,
            physics_tick_rate: 64,
            reconcile_position_rate: 20,
            ai_tick_rate: 20,
        }
    }
}

impl GameLoop {
    pub fn broadcast_positions(&self) -> bool {
        self.tick.is_multiple_of(self.reconcile_position_rate)
    }
}

#[derive(Default)]
struct SharedStacks {
    movement: HashMap<Entity, MoveBundle>,
}

const MAX_CHUNKS_PER_TICK_PER_CLIENT: usize = 12;

#[derive(Default)]
struct ClientStates {
    clients: HashMap<NetworkId, Entity>,
    clients_inverted: HashMap<Entity, NetworkId>,
    seen_chunks: HashMap<NetworkId, HashSet<Vec2iChunk>>,
    positions: HashMap<NetworkId, Vec2iChunk>,
}

#[derive(Default)]
struct EntityStates {
    entities: HashMap<NetworkId, Entity>,
    entities_inverted: HashMap<Entity, NetworkId>,
}

impl EntityStates {
    pub fn iter(&self) -> impl Iterator<Item = (&NetworkId, &Entity)> + '_ {
        self.entities.iter()
    }

    pub fn get(&self, entity: &Entity) -> Option<&NetworkId> {
        self.entities_inverted.get(entity)
    }

    pub fn insert(&mut self, network_id: NetworkId, entity: Entity) {
        self.entities.insert(network_id, entity);
        self.entities_inverted.insert(entity, network_id);
    }

    pub fn remove(&mut self, entity: &Entity) -> Option<NetworkId> {
        if let Some(network_id) = self.entities_inverted.remove(entity) {
            self.entities.remove(&network_id);
            Some(network_id)
        } else {
            None
        }
    }
}

impl ClientStates {
    pub fn iter(&self) -> impl Iterator<Item = (&NetworkId, &Entity)> + '_ {
        self.clients.iter()
    }

    pub fn all(&self) -> impl Iterator<Item = NetworkId> + '_ {
        self.clients.keys().copied()
    }
    pub fn all_pos(&self) -> impl Iterator<Item = Vec2iChunk> + '_ {
        self.positions.values().copied()
    }

    pub fn get2(&self, client_id: &NetworkId) -> Option<&Entity> {
        self.clients.get(client_id)
    }

    pub fn get(&self, entity: &Entity) -> Option<&NetworkId> {
        self.clients_inverted.get(entity)
    }

    pub fn update(&mut self, client_id: NetworkId, position: Vec2iChunk) {
        self.positions.insert(client_id, position);
    }

    pub fn insert(&mut self, client_id: NetworkId, entity: Entity, position: Vec2iChunk) {
        self.clients.insert(client_id, entity);
        self.clients_inverted.insert(entity, client_id);
        self.positions.insert(client_id, position);
    }

    pub fn remove(&mut self, client_id: &NetworkId) -> Option<Entity> {
        self.seen_chunks.remove(client_id);
        self.positions.remove(client_id);
        if let Some(entity) = self.clients.remove(client_id) {
            self.clients_inverted.remove(&entity);
            Some(entity)
        } else {
            None
        }
    }
}

impl GameServer {
    fn run_tick_system(&mut self, ticks: u64) {
        self.time_of_day.advance(ticks);

        let msg = WorldMessage::ServerTime(self.time_of_day);
        msg.transmit(&mut self.server, self.client_states.all(), [], None);

        if self
            .game_loop
            .tick
            .is_multiple_of(self.game_loop.physics_tick_rate)
        {
            // Physics tick logic TODO
        }

        if self.game_loop.tick.is_multiple_of(self.game_loop.ai_tick_rate) {
            self.tick_ai();
        }

        if self
            .game_loop
            .tick
            .is_multiple_of(self.game_loop.chunk_sweep_rate)
        {
            let swept = self.sweep_chunks();
            if swept > 0 {
                tracing::debug!(
                    "Chunk cleanup: {swept} ({} currently loaded)",
                    self.chunks.chunk_count(),
                );
            }
        }

        self.game_loop.tick += ticks;
    }
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
        let server = RenetServer::new(Default::default());

        Ok(Self {
            server,
            transport,
            world: GameWorld::new(generator),
            chunks: ChunkMap::new_persistent("world/chunks")?,
            time_of_day: TimeOfDay::default(),
            resource_pack: ResourcePack::load(),
            client_states: Default::default(),
            entity_states: Default::default(),
            stacks: Default::default(),
            game_loop: GameLoop::default(),
            emitter_seed_counter: Default::default(),
            entity_id_counter: Default::default(),
        })
    }

    pub fn update(&mut self, dt: Duration) -> Result<(), Error> {
        self.server.update(dt);

        if let Err(e) = self.transport.update(dt, &mut self.server) {
            anyhow::bail!("Transport update failed: {e}");
        }

        self.process_events()?;
        self.receive_messages()?;

        self.run_tick_system(1);

        self.stream_chunks_for_players()?;

        ecs::movement::apply_intent_all(
            self.world.world_mut(),
            &mut self.stacks.movement,
            &self.chunks,
            &self.resource_pack,
            self.game_loop.broadcast_positions(),
            dt,
        );

        if let Err(err) = self.chunks.persist_dirty(24) {
            tracing::warn!("Failed to enqueue dirty chunk persistence: {err:#}");
        }

        self.chunks.poll_persistence();

        self.emit_entities();

        self.transport.send_packets(&mut self.server);

        Ok(())
    }

    fn tick_ai(&mut self) {
        let mut pathfinding = self.world.world_mut().query_filtered::<(
            Entity,
            &mut NpcController,
            &EntityPosition,
            &EntityOrientation,
        ), With<Pathfinding>>();

        for (entity, mut controller, position, orientation) in
            pathfinding.query_mut(self.world.world_mut())
        {
            let Some(entity_id) = self.entity_states.get(&entity).copied() else {
                tracing::warn!("Entity {entity:?} has no network id");
                continue;
            };

            let msgs = pathfinding_tick(
                entity_id,
                &mut controller,
                *position,
                *orientation,
                self.game_loop.tick,
                &self.chunks,
            );

            match msgs.len() {
                0 => {}
                1 => {
                    let origin_chunk = Vec2iChunk::from(position.0);
                    msgs[0].transmit(
                        &mut self.server,
                        self.client_states.all(),
                        self.client_states.all_pos(),
                        Some(origin_chunk),
                    );
                }
                _ => {
                    let origin_chunk = Vec2iChunk::from(position.0);
                    msgs[0].transmit_multiple(
                        msgs.into_iter().skip(1),
                        &mut self.server,
                        self.client_states.all(),
                        self.client_states.all_pos(),
                        Some(origin_chunk),
                    )
                }
            }
        }
    }

    fn stream_chunks_for_players(&mut self) -> Result<(), Error> {
        for (client_id, chunk) in self.client_states.positions.clone() {
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

        let msg = WorldMessage::BlockModification {
            position: block_position,
            before,
            after: block_id,
        };

        let recipients: Vec<_> = self
            .client_states
            .all()
            .filter_map(|client_id| {
                self.client_states
                    .seen_chunks
                    .get(&client_id)
                    .and_then(|seen_chunks| {
                        if seen_chunks.contains(&chunk_coordinate) {
                            Some(client_id)
                        } else {
                            None
                        }
                    })
            })
            .collect();

        let server = &mut self.server;
        let seen_chunks = &mut self.client_states.seen_chunks;
        msg.transmit_callback(server, recipients, [], None, |id| {
            seen_chunks.entry(id).or_default().insert(chunk_coordinate);
        });

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

        let msg = WorldMessage::ParticleSpawn { emitter };

        msg.transmit(
            &mut self.server,
            self.client_states.all(),
            self.client_states.all_pos(),
            Some(chunk_coordinate),
        );

        let id = self.entity_id();
        let entity = self
            .world
            .spawn(
                &mut self.server,
                self.client_states.all(),
                id,
                NpcBundle::new(
                    EntityBundle::new(
                        EntityPosition((block_position + [0, 3, 0].into()).into()),
                        Default::default(),
                        Default::default(),
                        Default::default(),
                        Default::default(),
                        BoxCollider(spatial::aabb::BoxCollider::for_model(
                            ModelDefinition::Quadruped,
                        )),
                        EntityModel::for_model(ModelDefinition::Quadruped),
                        Default::default(),
                    ),
                    NpcController::default(),
                ),
            )
            .unwrap();

        self.entity_states.insert(id, entity.id());

        Ok(())
    }

    fn sweep_chunks(&mut self) -> usize {
        let positions: Vec<_> = self.client_states.all_pos().collect();

        if !positions.is_empty() {
            self.world
                .unload_distant_chunks(&mut self.chunks, &positions, 2 * RENDER_DISTANCE)
        } else {
            0
        }
    }

    fn emit_entities(&mut self) {
        for (moved, move_bundle) in self.stacks.movement.drain() {
            let Some(&entity_id) = self
                .client_states
                .get(&moved)
                .or_else(|| self.entity_states.get(&moved))
            else {
                tracing::warn!("Entity moved but has no network id");
                continue;
            };

            let origin_chunk = Vec2iChunk::from(move_bundle.position());

            if self.client_states.clients.contains_key(&entity_id) {
                self.client_states.update(entity_id, origin_chunk);
            }

            let move_msg = match (move_bundle.velocity(), move_bundle.collision()) {
                (Some(velocity), Some(collision_status)) => Some(EntityMessage::Move {
                    entity_id,
                    position: EntityPosition(move_bundle.position()),
                    velocity,
                    collision_status,
                }),
                _ => None,
            };

            let look_msg = move_bundle.orientation().map(|orientation| EntityMessage::Look {
                entity_id,
                orientation,
            });

            match (move_msg, look_msg) {
                (Some(move_msg), Some(look_msg)) => {
                    move_msg.transmit_multiple(
                        [look_msg],
                        &mut self.server,
                        self.client_states.all(),
                        self.client_states.all_pos(),
                        Some(origin_chunk),
                    );
                }
                (Some(msg), None) | (None, Some(msg)) => {
                    msg.transmit(
                        &mut self.server,
                        self.client_states.all(),
                        self.client_states.all_pos(),
                        Some(origin_chunk),
                    );
                }
                _ => {}
            }
        }
    }

    pub fn entity_id(&mut self) -> NetworkId {
        self.entity_id_counter = self.entity_id_counter.wrapping_add(1);
        NetworkId(self.entity_id_counter)
    }

    fn process_events(&mut self) -> Result<(), Error> {
        while let Some(event) = self.server.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    let bundle = EntityBundle::new(
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

                    let entity_mut = self.world.spawn(
                        &mut self.server,
                        self.client_states.all(),
                        client_id,
                        bundle,
                    )?;
                    let entity = entity_mut.id();

                    self.client_states
                        .insert(client_id, entity, Vec2iChunk::from(bundle.position.0));

                    self.sync_existing_entities(
                        client_id,
                        bundle.position,
                        bundle.collider,
                        bundle.model,
                    )?;
                }
                ServerEvent::ClientDisconnected { client_id, .. } => {
                    let client_id = NetworkId(client_id);
                    if let Some(entity) = self.client_states.remove(&client_id) {
                        self.world
                            .despawn(&mut self.server, self.client_states.all(), client_id, entity);
                    }
                }
            }
        }

        Ok(())
    }

    fn receive_messages(&mut self) -> Result<(), Error> {
        for client_id in self.server.clients_id() {
            let network_id = NetworkId(client_id);
            let Some(&entity) = self.client_states.get2(&network_id) else {
                continue;
            };

            for channel in ClientMessage::receive_channels() {
                while let Some(msg) = self.server.receive_message(client_id, channel) {
                    let msg = ClientMessage::decode(&msg)?;

                    match msg {
                        ClientMessage::RemodelEntity {
                            model,
                            bounding_box,
                        } => {
                            let mut query = self.world.world_mut().entity_mut(entity);
                            let Ok((mut entity_model, mut entity_collider, position)) = query
                                .get_components_mut::<(
                                    &mut EntityModel,
                                    &mut BoxCollider,
                                    &EntityPosition,
                                )>()
                            else {
                                continue;
                            };

                            *entity_model = model;
                            *entity_collider = bounding_box;

                            let origin_chunk = Vec2iChunk::from(position.0);

                            let msg = EntityMessage::Remodel {
                                entity_id: network_id,
                                model,
                                bounding_box,
                            };

                            msg.transmit_except(
                                &mut self.server,
                                network_id,
                                self.client_states.all(),
                                self.client_states.all_pos(),
                                Some(origin_chunk),
                            );
                        }
                        ClientMessage::InteractBlock {
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

                            let base_properties =
                                ModelDefinition::from_handle(model.model_id).properties();

                            let ray = raycast(
                                ray_origin,
                                orientation.0,
                                base_properties.reach_distance,
                                &self.chunks,
                                &self.resource_pack,
                            );

                            if let Some((block, _, normal)) = ray {
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
                        ClientMessage::Move(intent) => {
                            let Some(mut current_intent) =
                                self.world.world_mut().get_mut::<MovementIntent>(entity)
                            else {
                                continue;
                            };
                            *current_intent = intent;
                        }
                        ClientMessage::Look(orientation) => {
                            let mut query = self.world.world_mut().entity_mut(entity);

                            let Ok((mut current_orientation, position)) =
                                query.get_components_mut::<(&mut EntityOrientation, &EntityPosition)>()
                            else {
                                continue;
                            };

                            if *current_orientation == orientation {
                                continue;
                            }
                            *current_orientation = orientation;

                            let origin_chunk = Vec2iChunk::from(position.0);

                            let msg = EntityMessage::Look {
                                entity_id: network_id,
                                orientation,
                            };

                            msg.transmit_except(
                                &mut self.server,
                                network_id,
                                self.client_states.all(),
                                self.client_states.all_pos(),
                                Some(origin_chunk),
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Tell a newly connected client about all existing entities within render distance
    fn sync_existing_entities(
        &mut self,
        client_id: NetworkId,
        position: EntityPosition,
        bounding_box: BoxCollider,
        model: EntityModel,
    ) -> Result<(), Error> {
        let msg = EntityMessage::ClientConnect {
            entity_id: client_id,
            position,
            bounding_box,
            model,
        };
        msg.transmit(&mut self.server, [client_id], [], None);

        let observer_chunk = Vec2iChunk::from(position.0);

        for (other_client_id, &position, &bounding_box, &model) in self
            .client_states
            .iter()
            .chain(self.entity_states.iter())
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
            let msg = EntityMessage::Spawn {
                entity_id: other_client_id,
                position,
                bounding_box,
                model,
            };

            msg.transmit(&mut self.server, [client_id], [], None);
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

        let sent = self.client_states.seen_chunks.entry(client_id).or_default();
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
                let msg = WorldMessage::ChunkData(chunk);

                msg.transmit_callback(&mut self.server, [client_id], [], None, |_| {
                    sent.insert(coordinate);
                    sent_this_call += 1;
                });
            }
        }

        Ok(())
    }
}
