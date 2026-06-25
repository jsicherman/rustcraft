use chunk::ChunkProvider;
use ecs::{
    BoxCollider, CollisionStatus, EntityModel, EntityOrientation, EntityPosition, EntityVelocity,
    SimulatedEntityBundle,
};
use protocol::{CHANNEL_CHUNKS, CHANNEL_ENTITIES, Packet, RENDER_DISTANCE_SQ, ServerMessage};
use render::model::RenderHandle;
use smallvec::SmallVec;
use spatial::vectors::{Chunk, IntoSpace, Vec2iChunk, Vec3fGlobal};

use crate::{AppState, renderer::NetworkRenderable};

impl AppState {
    pub(crate) fn receive_chunk_messages(&mut self, chunk_position: Vec2iChunk) {
        let mut meshes_to_remove = SmallVec::<[_; 16]>::new();

        self.chunk_state.retain_chunks(|coord, chunk| {
            let retained = (*coord - chunk_position).length_sq() <= RENDER_DISTANCE_SQ;

            if !retained && let Some(instance) = chunk.instance() {
                let RenderHandle::Mesh(mesh) = instance.handle() else {
                    unreachable!();
                };
                meshes_to_remove.push(mesh);
            }

            retained
        });

        for mesh in meshes_to_remove {
            // chunks are only singly-referenced, so drop the mesh when the chunk is unloaded
            self.renderer.remove_mesh(&mesh);
        }

        while let Some(msg) = self.client.receive_message(CHANNEL_CHUNKS) {
            let msg = ServerMessage::decode(&msg).unwrap();

            match msg {
                ServerMessage::ChunkData(chunk) => {
                    let coordinate = chunk.coordinate();

                    if (coordinate - chunk_position).length_sq() > RENDER_DISTANCE_SQ {
                        continue;
                    }

                    self.chunk_state.insert_wire_chunk(chunk);
                }
                ServerMessage::BlockEdit {
                    position,
                    before,
                    after,
                } => {
                    let chunk_coordinate = IntoSpace::<Chunk>::into_space(position);
                    let coordinate = Vec2iChunk::from([chunk_coordinate[0], chunk_coordinate[2]]);

                    if (coordinate - chunk_position).length_sq() > RENDER_DISTANCE_SQ {
                        continue;
                    }

                    // FIXME: get this when you break it yourself
                    if self
                        .chunk_state
                        .block(position.into())
                        .map(|block| block.id())
                        != Some(before)
                    {
                        tracing::warn!("mismatch at {position:?}: expected={before:?}");
                    }

                    if self.chunk_state.set_block(position.into(), after).is_none() {
                        continue;
                    }

                    self.invalidate_chunk_meshes_around_block(position);
                }
                ServerMessage::EntityMove { .. }
                | ServerMessage::EntityLook { .. }
                | ServerMessage::EntitySpawn { .. }
                | ServerMessage::EntityDespawn(_)
                | ServerMessage::ClientSpawned(_)
                | ServerMessage::EntityRemodel { .. }
                | ServerMessage::ParticleSpawn { .. }
                | ServerMessage::ServerTime(_) => {
                    unreachable!()
                }
            }
        }
    }

    pub(crate) fn receive_entity_messages(&mut self, _current_position: Vec3fGlobal) {
        while let Some(msg) = self.client.receive_message(CHANNEL_ENTITIES) {
            let msg = ServerMessage::decode(&msg).unwrap();

            match msg {
                ServerMessage::ChunkData(_) | ServerMessage::BlockEdit { .. } => unreachable!(),
                ServerMessage::ServerTime(time) => {
                    self.time_of_day = time;
                }
                ServerMessage::ParticleSpawn { emitter } => {
                    self.particles.spawn(emitter);
                }
                ServerMessage::ClientSpawned(entity_id) => {
                    if let Some(entity) = self.network_to_local.get(&entity_id) {
                        self.local_player = Some((entity_id, Some(*entity)));
                    } else {
                        self.local_player = Some((entity_id, None));
                    }
                }
                ServerMessage::EntitySpawn {
                    entity_id,
                    position,
                    bounding_box,
                    model,
                } => {
                    let spawned_id = self
                        .world
                        .spawn(SimulatedEntityBundle::new(
                            position,
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            bounding_box,
                            model,
                            Default::default(),
                        ))
                        .id();

                    if let Some((id, entity)) = self.local_player.as_mut()
                        && *id == entity_id
                        && entity.is_none()
                    {
                        *entity = Some(spawned_id);
                    }

                    self.network_to_local.insert(entity_id, spawned_id);
                    // self.request_entity_frame(spawned_id, current_position);
                }
                ServerMessage::EntityDespawn(entity_id) => {
                    if let Some(entity) = self.network_to_local.remove(&entity_id) {
                        self.world.despawn(entity);
                        self.entity_state.remove_instance(&entity);
                    }

                    if let Some((id, _)) = self.local_player
                        && id == entity_id
                    {
                        self.local_player = None;
                    }
                }
                ServerMessage::EntityMove {
                    entity_id,
                    position,
                    velocity,
                    collision_status,
                } => {
                    let Some(entity_id) = self.network_to_local.get(&entity_id) else {
                        continue;
                    };

                    let Ok(mut entity) = self.world.get_entity_mut(*entity_id) else {
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

                    if let Some((_, Some(local_entity))) = self.local_player
                        && local_entity == *entity_id
                    {
                        let delta_p = (position.0 - client_position.0).length_sq();
                        if delta_p > 0.1 {
                            let delta_v = (velocity.0 - client_velocity.0).length_sq();
                            tracing::debug!(
                                "Large player offset! delta_p = {delta_p:.4}, delta_v = {delta_v:.4}",
                            );
                        }
                    }

                    *client_position = position;
                    *client_velocity = velocity;
                    *client_collision_status = collision_status;

                    // self.request_entity_frame(*entity_id, current_position);
                }
                ServerMessage::EntityLook {
                    entity_id,
                    orientation,
                } => {
                    let Some(entity_id) = self.network_to_local.get(&entity_id) else {
                        continue;
                    };

                    let Ok(mut entity) = self.world.get_entity_mut(*entity_id) else {
                        continue;
                    };

                    if let Some(mut client_orientation) = entity.get_mut::<EntityOrientation>() {
                        *client_orientation = orientation;
                    }

                    // self.request_entity_frame(*entity_id, current_position);
                }
                ServerMessage::EntityRemodel {
                    entity_id,
                    model,
                    bounding_box,
                } => {
                    let Some(entity_id) = self.network_to_local.get(&entity_id) else {
                        continue;
                    };

                    let Ok(mut entity) = self.world.get_entity_mut(*entity_id) else {
                        continue;
                    };

                    if let Ok((mut client_model, mut client_bounding_box)) =
                        entity.get_components_mut::<(&mut EntityModel, &mut BoxCollider)>()
                    {
                        *client_model = model;
                        *client_bounding_box = bounding_box;
                    }

                    // self.request_entity_frame(*entity_id, current_position);
                }
            }
        }
    }
}
