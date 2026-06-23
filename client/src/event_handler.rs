use ecs::{
    CollisionStatus, EntityOrientation, EntityPosition, EntityVelocity, SimulatedEntityBundle,
};
use protocol::{CHANNEL_CHUNKS, CHANNEL_ENTITIES, Packet, RENDER_DISTANCE_SQ, ServerMessage};
use spatial::vectors::Vec2iChunk;

use crate::{
    AppState,
    world::{ClientChunk, ClientRenderable},
};

impl AppState {
    pub(crate) fn receive_chunks(&mut self, chunk_position: Vec2iChunk) {
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
                | ServerMessage::EntityDespawn(_)
                | ServerMessage::ClientSpawned(_) => {
                    unreachable!()
                }
            }
        }
    }

    pub(crate) fn receive_entities(&mut self, _chunk_position: Vec2iChunk) {
        while let Some(msg) = self.client.receive_message(CHANNEL_ENTITIES) {
            let msg = ServerMessage::decode(&msg).unwrap();

            tracing::debug!("received {:?}", msg);

            match msg {
                ServerMessage::ChunkData(_) => unreachable!(),
                ServerMessage::ClientSpawned(entity_id) => {
                    tracing::debug!("server identified us as {:?}", entity_id);
                    self.local_player_network_id = Some(entity_id);
                }
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
                        client_entity.mark_dirty();
                    }
                }
                ServerMessage::EntityLook {
                    entity_id,
                    orientation,
                } => {
                    if self.local_player_network_id == Some(entity_id) {
                        continue;
                    }

                    let Some(entity) = self.network_to_local.get(&entity_id) else {
                        continue;
                    };

                    let Ok(mut entity) = self.world.get_entity_mut(*entity) else {
                        continue;
                    };

                    if let Some(mut client_orientation) = entity.get_mut::<EntityOrientation>() {
                        let orientation_changed = *client_orientation != orientation;
                        *client_orientation = orientation;

                        if orientation_changed
                            && let Some(client_entity) =
                                self.loaded_entities.entities.get_mut(&entity_id)
                        {
                            client_entity.mark_dirty();
                        }
                    }
                }
                ServerMessage::EntitySpawn {
                    entity_id,
                    position,
                } => {
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
                    if self.local_player_network_id == Some(entity_id) {
                        self.local_player_network_id = None;
                    }
                }
            }
        }
    }
}
