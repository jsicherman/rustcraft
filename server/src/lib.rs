mod world;

use anyhow::Error;
use block::BlockRegistry;
use chunk::ChunkMap;
use ecs::{
    Entity, EntityOrientation, EntityPosition, MovementIntent, SimulatedEntityBundle,
    movement::MoveBundle,
};
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
    entities: HashMap<u64, Entity>,
    entities_inverted: HashMap<Entity, u64>,
    client_states: ClientStates,
    chunk_sweep_timer: Duration,
    stacks: SharedStacks,
}

#[derive(Default)]
struct SharedStacks {
    movement: HashMap<Entity, MoveBundle>,
    chunk_receivers: Vec<u64>,
}

const CHUNK_SWEEP_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default)]
struct ClientStates {
    sent_chunks: HashMap<u64, HashSet<Vec2iChunk>>,
    player_positions: HashMap<u64, Vec2iChunk>,
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

        self.sweep_chunks(dt);

        self.transport.send_packets(&mut self.server);
        Ok(())
    }

    fn sweep_chunks(&mut self, dt: Duration) {
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
                    .unload_distant_chunks(&mut self.chunks, &positions, 2 * RENDER_DISTANCE);
            }
        }
    }

    fn emit_entities(&mut self) {
        for (moved, move_bundle) in self.stacks.movement.drain() {
            let Some(entity_id) = self.entities_inverted.get(&moved) else {
                tracing::warn!("Entity moved but has no network id");
                continue;
            };

            let origin_chunk = Vec2iChunk::from(move_bundle.position());

            let move_msg = match (move_bundle.velocity(), move_bundle.collision()) {
                (Some(velocity), Some(collision_status)) => {
                    let move_msg = ServerMessage::EntityMove {
                        entity_id: NetworkId(*entity_id),
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
                    entity_id: NetworkId(*entity_id),
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
                        .send_message(*client_id, CHANNEL_ENTITIES, entity_move);
                }
                if let Some(entity_look) = look_msg.clone() {
                    self.server
                        .send_message(*client_id, CHANNEL_ENTITIES, entity_look);
                }
            }
        }
    }

    fn process_events(&mut self) -> Result<(), Error> {
        while let Some(event) = self.server.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    let bundle = SimulatedEntityBundle::default();

                    let entity = self
                        .world
                        .spawn(
                            &mut self.server,
                            self.entities
                                .keys()
                                .copied()
                                .chain(std::iter::once(client_id)),
                            NetworkId(client_id),
                            bundle,
                        )?
                        .id();

                    self.entities.insert(client_id, entity);
                    self.entities_inverted.insert(entity, client_id);

                    self.client_states
                        .sent_chunks
                        .insert(client_id, Default::default());

                    self.sync_existing_entities(client_id)?;

                    self.broadcast_chunks(client_id)?;
                }
                ServerEvent::ClientDisconnected { client_id, .. } => {
                    if let Some(entity) = self.entities.remove(&client_id) {
                        self.world.despawn(
                            &mut self.server,
                            self.entities.keys().copied(),
                            NetworkId(client_id),
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
            let Some(entity) = self.entities.get_mut(&client_id) else {
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

                        self.stacks.chunk_receivers.push(client_id);
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

                        let Some(&position) = self.world.world().get::<EntityPosition>(*entity)
                        else {
                            continue;
                        };

                        let origin_chunk = Vec2iChunk::from(position.0);
                        let msg = ServerMessage::EntityLook {
                            entity_id: NetworkId(client_id),
                            orientation,
                        }
                        .encode()?;

                        for (observer_id, _) in self
                            .client_states
                            .player_positions
                            .iter()
                            // don't replay the look message back to the client that sent it
                            .filter(|(observer_id, _)| *observer_id != &client_id)
                            .filter(|(_, chunk)| {
                                (**chunk - origin_chunk).length_sq() <= RENDER_DISTANCE_SQ
                            })
                        {
                            self.server
                                .send_message(*observer_id, CHANNEL_ENTITIES, msg.clone());
                        }
                    }
                }
            }
        }

        while let Some(client_id) = self.stacks.chunk_receivers.pop() {
            self.broadcast_chunks(client_id)?;
        }

        Ok(())
    }

    fn sync_existing_entities(&mut self, client_id: u64) -> Result<(), Error> {
        let Some(observer_entity) = self.entities.get(&client_id).copied() else {
            return Ok(());
        };

        let msg = ServerMessage::ClientSpawned(NetworkId(client_id)).encode()?;
        self.server.send_message(client_id, CHANNEL_ENTITIES, msg);

        let Some(observer_position) = self
            .world
            .world()
            .get::<EntityPosition>(observer_entity)
            .copied()
        else {
            return Ok(());
        };

        let observer_chunk = Vec2iChunk::from(observer_position.0);

        let existing: Vec<(u64, EntityPosition)> = self
            .entities
            .iter()
            .filter_map(|(other_client_id, other_entity)| {
                if *other_client_id == client_id {
                    return None;
                }

                let position = self
                    .world
                    .world()
                    .get::<EntityPosition>(*other_entity)
                    .copied()?;
                let other_chunk = Vec2iChunk::from(position.0);

                if (other_chunk - observer_chunk).length_sq() > RENDER_DISTANCE_SQ {
                    return None;
                }

                Some((*other_client_id, position))
            })
            .collect();

        for (other_client_id, position) in existing {
            let msg = ServerMessage::EntitySpawn {
                entity_id: NetworkId(other_client_id),
                position,
            }
            .encode()?;

            self.server.send_message(client_id, CHANNEL_ENTITIES, msg);
        }

        Ok(())
    }

    fn broadcast_chunks(&mut self, client_id: u64) -> Result<(), Error> {
        let Some(entity) = self.entities.get(&client_id).copied() else {
            return Ok(());
        };

        let Some(position) = self.world.world().get::<EntityPosition>(entity).copied() else {
            return Ok(());
        };

        let origin_chunk = Vec2iChunk::from(position.0);

        self.client_states
            .player_positions
            .insert(client_id, origin_chunk);
        self.send_nearby_chunks(client_id, origin_chunk, RENDER_DISTANCE)
    }

    fn send_nearby_chunks(
        &mut self,
        client_id: u64,
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

                self.server.send_message(client_id, CHANNEL_CHUNKS, msg);
                sent.insert(coordinate);
            }
        }

        Ok(())
    }
}
