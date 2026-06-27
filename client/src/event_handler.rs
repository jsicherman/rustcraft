use chunk::ChunkProvider;
use ecs::{
    BoxCollider, CollisionStatus, EntityBundle, EntityModel, EntityOrientation, EntityPosition,
    EntityVelocity, LocalPlayer, Pathfinding,
};
use protocol::{
    Packet, RENDER_DISTANCE_SQ, ServerMessage, entity::EntityMessage, world::WorldMessage,
};
use render::model::RenderHandle;
use smallvec::SmallVec;
use spatial::vectors::{Chunk, IntoSpace, Vec2iChunk, Vec3fGlobal};

use crate::{AppState, frame_handler::reconcile, renderer::NetworkRenderable};

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

        while let Some(msg) = self.client.receive_message(WorldMessage::channel()) {
            let msg = WorldMessage::decode(&msg).unwrap();
            let ServerMessage::World(msg) = msg else {
                unreachable!();
            };

            match msg {
                WorldMessage::ChunkData(chunk) => {
                    let coordinate = chunk.coordinate();

                    if (coordinate - chunk_position).length_sq() > RENDER_DISTANCE_SQ {
                        continue;
                    }

                    self.chunk_state.insert_wire_chunk(chunk);
                }
                WorldMessage::BlockModification {
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
                WorldMessage::ServerTime(time) => {
                    self.time_of_day = time;
                }
                WorldMessage::ParticleSpawn { emitter } => {
                    self.particles.spawn(emitter);
                }
            }
        }
    }

    pub(crate) fn receive_entity_messages(&mut self, _current_position: Vec3fGlobal) {
        for channel in EntityMessage::receive_channels() {
            while let Some(msg) = self.client.receive_message(channel) {
                let msg = EntityMessage::decode(&msg).unwrap();
                let ServerMessage::Entity(msg) = msg else {
                    unreachable!();
                };

                match msg {
                    EntityMessage::ClientConnect {
                        entity_id,
                        position,
                        bounding_box,
                        model,
                    } => {
                        let spawned_id = self
                            .world
                            .spawn((
                                EntityBundle::new(
                                    position,
                                    Default::default(),
                                    Default::default(),
                                    Default::default(),
                                    Default::default(),
                                    bounding_box,
                                    model,
                                    Default::default(),
                                ),
                                LocalPlayer,
                            ))
                            .id();

                        self.local_player = Some((entity_id, Some(spawned_id)));
                        self.network_to_local.insert(entity_id, spawned_id);
                    }
                    EntityMessage::Despawn(entity_id) => {
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
                    EntityMessage::Spawn {
                        entity_id,
                        position,
                        bounding_box,
                        model,
                    } => {
                        let spawned_id = self
                            .world
                            .spawn((
                                EntityBundle::new(
                                    position,
                                    Default::default(),
                                    Default::default(),
                                    Default::default(),
                                    Default::default(),
                                    bounding_box,
                                    model,
                                    Default::default(),
                                ),
                                // TODO: need to send the sentinel
                                Pathfinding,
                            ))
                            .id();

                        if let Some((id, entity)) = self.local_player.as_mut()
                            && *id == entity_id
                            && entity.is_none()
                        {
                            *entity = Some(spawned_id);
                        }

                        self.network_to_local.insert(entity_id, spawned_id);
                    }
                    EntityMessage::Move {
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

                        let Ok((
                            mut client_position,
                            mut client_velocity,
                            mut client_collision_status,
                        )) = entity.get_components_mut::<(
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
                            let error = (position.0 - client_position.0).length();

                            if error > 0.1 {
                                let delta_v = (velocity.0 - client_velocity.0).length_sq();
                                tracing::debug!(
                                    "Large player offset! error = {error:.4}, delta_v = {delta_v:.4}",
                                );
                            }

                            let (rp, rv) = reconcile(
                                (*client_position, *client_velocity),
                                (position, velocity),
                                error,
                                4.0,
                                // FIXME: probably depend on movement speed
                                0.05,
                                ((error / 4.0) * 0.4).clamp(0.01, 0.25),
                            );

                            *client_position = rp;
                            *client_velocity = rv;
                        } else {
                            *client_position =
                                EntityPosition(client_position.0.lerp(position.0, 0.5));
                            *client_velocity = velocity;
                        }

                        *client_collision_status = collision_status;
                    }
                    EntityMessage::Look {
                        entity_id,
                        orientation,
                    } => {
                        let Some(entity_id) = self.network_to_local.get(&entity_id) else {
                            continue;
                        };

                        let Ok(mut entity) = self.world.get_entity_mut(*entity_id) else {
                            continue;
                        };

                        if let Some(mut client_orientation) = entity.get_mut::<EntityOrientation>()
                        {
                            *client_orientation = orientation;
                        }
                    }
                    EntityMessage::Remodel {
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
                    }
                    EntityMessage::BlockEntityUpdate { position, data } => todo!(),
                    EntityMessage::GuidedMove {
                        entity_id,
                        movement,
                    } => {
                        // TODO
                    }
                    EntityMessage::GuidedLook {
                        entity_id,
                        orientation,
                    } => {
                        // TODO
                    }
                }
            }
        }
    }
}
