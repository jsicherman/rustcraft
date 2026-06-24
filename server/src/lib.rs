mod world;

use anyhow::Error;
use block::BlockRegistry;
use chunk::ChunkMap;
use ecs::{
    BoxCollider, Entity, EntityModel, EntityOrientation, EntityPosition, MovementIntent,
    SimulatedEntityBundle, movement::MoveBundle,
};
use model::ModelDefinition;
use protocol::{
    CHANNEL_CHUNKS, CHANNEL_ENTITIES, ClientMessage, NetworkId, PROTOCOL_ID, Packet,
    RENDER_DISTANCE, RENDER_DISTANCE_SQ, ServerMessage,
};
use renet::{ConnectionConfig, RenetServer, ServerEvent};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};
use spatial::vectors::Vec2iChunk;
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, UdpSocket},
    time::{Duration, SystemTime},
};

pub use world::{DefaultWorldGenerator, WorldGenerator};

use crate::world::GameWorld;

pub struct GameServer<G: WorldGenerator> {
    server: RenetServer,
    transport: NetcodeServerTransport,
    world: GameWorld<G>,
    chunks: ChunkMap,
    block_registry: BlockRegistry,
    entities: HashMap<NetworkId, Entity>,
    entities_inverted: HashMap<Entity, NetworkId>,
    client_states: ClientStates,
    chunk_sweep_timer: Duration,
    stacks: SharedStacks,
}

#[derive(Default)]
struct SharedStacks {
    movement: HashMap<Entity, MoveBundle>,
    chunk_receivers: Vec<(NetworkId, Entity)>,
}

const CHUNK_SWEEP_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default)]
struct ClientStates {
    sent_chunks: HashMap<NetworkId, HashSet<Vec2iChunk>>,
    player_positions: HashMap<NetworkId, Vec2iChunk>,
}

impl<G: WorldGenerator> GameServer<G> {
    pub fn new(
        bind_addr: SocketAddr,
        public_addr: SocketAddr,
        max_clients: usize,
        generator: G,
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
        let server = RenetServer::new(ConnectionConfig::default());

        Ok(Self {
            server,
            transport,
            world: GameWorld::new(generator),
            chunks: ChunkMap::new(),
            block_registry: BlockRegistry::load(),
            client_states: Default::default(),
            entities: Default::default(),
            entities_inverted: Default::default(),
            stacks: Default::default(),
            chunk_sweep_timer: Duration::ZERO,
        })
    }

    pub fn update(&mut self, dt: Duration) -> Result<(), Error> {
        self.server.update(dt);

        if let Err(e) = self.transport.update(dt, &mut self.server) {
            anyhow::bail!("Transport update failed: {e}");
        }

        self.process_events()?;
        self.receive_messages()?;

        ecs::movement::apply_intent_all(
            self.world.world_mut(),
            &mut self.stacks.movement,
            &self.chunks,
            &self.block_registry,
            dt,
        );
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
                        EntityPosition([0.0, 70.0, 0.0].into()),
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

                    self.sync_existing_entities(client_id, position)?;
                    self.broadcast_chunks(client_id, position)?;
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
            let Some(entity) = self.entities.get_mut(&network_id) else {
                continue;
            };

            // TODO: on new channel?
            while let Some(msg) = self.server.receive_message(client_id, CHANNEL_CHUNKS) {
                let msg = ClientMessage::decode(&msg)?;

                match msg {
                    ClientMessage::Move(intent) => {
                        let Some(mut current_intent) =
                            self.world.world_mut().get_mut::<MovementIntent>(*entity)
                        else {
                            continue;
                        };
                        *current_intent = intent;

                        self.stacks.chunk_receivers.push((network_id, *entity));
                    }
                    ClientMessage::Look(orientation) => {
                        let Some(mut current_orientation) =
                            self.world.world_mut().get_mut::<EntityOrientation>(*entity)
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

                        for (observer_id, _) in self
                            .client_states
                            .player_positions
                            .iter()
                            // don't replay the look message back to the client that sent it
                            .filter(|(observer_id, _)| *observer_id != &network_id)
                            .filter(|(_, chunk)| {
                                (**chunk - origin_chunk).length_sq() <= RENDER_DISTANCE_SQ
                            })
                        {
                            self.server
                                .send_message(**observer_id, CHANNEL_ENTITIES, msg.clone());
                        }
                    }
                }
            }
        }

        while let Some((client_id, entity)) = self.stacks.chunk_receivers.pop() {
            let Some(position) = self.world.world().get::<EntityPosition>(entity).copied() else {
                continue;
            };

            self.broadcast_chunks(client_id, position)?;
        }

        Ok(())
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

    /// Broadcast the chunks around a client to that client, given their position
    fn broadcast_chunks(
        &mut self,
        client_id: NetworkId,
        position: EntityPosition,
    ) -> Result<(), Error> {
        let origin_chunk = Vec2iChunk::from(position.0);
        self.send_nearby_chunks(client_id, origin_chunk, RENDER_DISTANCE)
    }

    fn send_nearby_chunks(
        &mut self,
        client_id: NetworkId,
        coordinate: Vec2iChunk,
        max_distance: i32,
    ) -> Result<(), Error> {
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

                let chunk = self.world.generate(&mut self.chunks, coordinate).to_owned();
                let msg = ServerMessage::ChunkData(Box::new(chunk)).encode()?;

                self.server.send_message(*client_id, CHANNEL_CHUNKS, msg);
                sent.insert(coordinate);
            }
        }

        Ok(())
    }
}
