use std::{
    collections::{HashSet, hash_map::Entry},
    time::{Duration, Instant},
};

use bevy_ecs::query::Without;
use chunk::{ChunkProvider, raycasting::raycast};
use ecs::{
    BoxCollider, CollisionStatus, EntityModel, EntityOrientation, EntityPosition, EntityVelocity,
    InteractionIntent, LocalPlayer, MovementIntent,
};
use protocol::{ServerBound, entity::ClientMessage};
use render::Renderer;
use renet::RenetClient;
use resources::{
    ResourcePack,
    entity::{EntityProperties, EntityType},
};
use resources::{block::BlockId, entity::ModelDefinition};
use spatial::{
    aabb::Aabb,
    orientation::Orientation,
    vectors::{Vec3fGlobal, Vec3iGlobal},
};
use winit::{event::MouseButton, keyboard::KeyCode};

use crate::{
    AppState, ChunkCache, PreviousState,
    camera::{INVERT_PITCH, INVERT_YAW, MOUSE_SENSITIVITY},
};

struct MouseInputContext<'a> {
    chunks: &'a mut ChunkCache,
    renderer: &'a mut Renderer,
    resource_pack: &'a ResourcePack,
    buttons: &'a HashSet<MouseButton>,
    client: &'a mut RenetClient,
    previous_state: &'a mut PreviousState,
    orientation: &'a mut EntityOrientation,
    model: &'a mut EntityModel,
    base_properties: &'a EntityProperties,
    bounding_box: &'a mut BoxCollider,
    interact_intent: &'a mut InteractionIntent,
}

impl AppState {
    fn resolve_remote_collision_snap(
        previous_position: Vec3fGlobal,
        velocity: Vec3fGlobal,
        collider: BoxCollider,
        previous_status: CollisionStatus,
        chunks: &impl ChunkProvider,
        resource_pack: &ResourcePack,
        dt: Duration,
    ) -> (Vec3fGlobal, Vec3fGlobal, CollisionStatus) {
        const SNAP_EPSILON: f32 = 1e-3;
        const MAX_PASSES: usize = 3;

        // Remote entities only simulate vertical motion locally.
        let mut velocity = Vec3fGlobal::new(0.0, velocity.y(), 0.0);
        let mut position = previous_position + velocity * dt.as_secs_f32();
        let mut status = previous_status;

        let swept = collider.0.aabb_swept(previous_position, velocity, dt);

        for _ in 0..MAX_PASSES {
            let mut resolved = false;
            let entity_aabb = collider.0.aabb(position);

            for block in chunks.intersecting(&swept) {
                let block_type = resource_pack.get_block_type(block.id());
                if !block_type.solidity().is_solid() {
                    continue;
                }

                let block_aabb = block.aabb(block_type);
                let is_overlap = entity_aabb.intersects_overlaps(&block_aabb);
                let crossed_swept = !is_overlap && swept.intersects_overlaps(&block_aabb);

                if !is_overlap && !crossed_swept {
                    continue;
                }

                let dy = position.y() - previous_position.y();
                let entity_center_x = (entity_aabb.min().x() + entity_aabb.max().x()) * 0.5;
                let entity_center_z = (entity_aabb.min().z() + entity_aabb.max().z()) * 0.5;
                let block_center_x = (block_aabb.min().x() + block_aabb.max().x()) * 0.5;
                let block_center_z = (block_aabb.min().z() + block_aabb.max().z()) * 0.5;

                let (left_x, right_x) = (
                    block_aabb.max().x() - entity_aabb.min().x(),
                    entity_aabb.max().x() - block_aabb.min().x(),
                );
                let (left_y, right_y) = (
                    block_aabb.max().y() - entity_aabb.min().y(),
                    entity_aabb.max().y() - block_aabb.min().y(),
                );
                let (left_z, right_z) = (
                    block_aabb.max().z() - entity_aabb.min().z(),
                    entity_aabb.max().z() - block_aabb.min().z(),
                );

                let overlap_x = left_x.min(right_x);
                let overlap_y = left_y.min(right_y);
                let overlap_z = left_z.min(right_z);

                if overlap_x <= overlap_y && overlap_x <= overlap_z {
                    if entity_center_x <= block_center_x {
                        position[0] = block_aabb.min().x() - collider.0.half_width() - SNAP_EPSILON;
                    } else {
                        position[0] = block_aabb.max().x() + collider.0.half_width() + SNAP_EPSILON;
                    }
                } else if overlap_y <= overlap_x && overlap_y <= overlap_z {
                    if dy >= 0.0 {
                        position[1] = block_aabb.min().y() - collider.0.height() - SNAP_EPSILON;
                    } else {
                        position[1] = block_aabb.max().y() + SNAP_EPSILON;
                        status = CollisionStatus::OnGround;
                    }
                    velocity[1] = 0.0;
                } else {
                    if entity_center_z <= block_center_z {
                        position[2] = block_aabb.min().z() - collider.0.half_width() - SNAP_EPSILON;
                    } else {
                        position[2] = block_aabb.max().z() + collider.0.half_width() + SNAP_EPSILON;
                    }
                }

                resolved = true;
                break;
            }

            if !resolved {
                break;
            }
        }

        if velocity.y() > 0.0 {
            status = CollisionStatus::Airborne;
        }

        (position, velocity, status)
    }

    pub(crate) fn invalidate_chunk_meshes_around_block(&mut self, block_position: Vec3iGlobal) {
        self.chunk_state
            .invalidate_chunk_meshes_around_block(&mut self.renderer, block_position);
    }

    pub(crate) fn process_gravity(&mut self, dt: Duration) {
        let chunks = &self.chunk_state;
        let resource_pack = &self.resource_pack;
        let world = &mut self.world;

        let mut query = world.query_filtered::<(
            &mut EntityVelocity,
            &mut EntityPosition,
            &mut CollisionStatus,
            &BoxCollider,
        ), Without<LocalPlayer>>();

        for (mut velocity, mut position, mut collision_status, collider) in query.iter_mut(world) {
            let previous_position = position.0;

            let (resolved_position, resolved_velocity, resolved_status) =
                Self::resolve_remote_collision_snap(
                    previous_position,
                    velocity.0,
                    *collider,
                    *collision_status,
                    chunks,
                    resource_pack,
                    dt,
                );

            *position = EntityPosition(resolved_position);
            *velocity = EntityVelocity(resolved_velocity);
            *collision_status = resolved_status;
        }
    }

    pub(crate) fn process_inputs(
        &mut self,
        dt: Duration,
    ) -> (Vec3fGlobal, Orientation, Vec3fGlobal, EntityProperties) {
        self.client.update(dt);

        match self.transport.update(dt, &mut self.client) {
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Transport error: {e}");
            }
        }

        let Some((_, Some(player_entity))) = self.local_player else {
            return (
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            );
        };

        let axis = |positive: KeyCode, negative: KeyCode| -> f32 {
            (self.pressed_keys.contains(&positive) as u8 as f32)
                - (self.pressed_keys.contains(&negative) as u8 as f32)
        };

        let forward = axis(KeyCode::KeyW, KeyCode::KeyS);
        let right = axis(KeyCode::KeyA, KeyCode::KeyD) * 0.8;

        let mut entity = self.world.entity_mut(player_entity);

        let (
            mut position,
            mut velocity,
            mut orientation,
            mut intent,
            mut mouse_intent,
            mut model,
            mut collider,
            mut collision_status,
        ) = entity
            .get_components_mut::<(
                &mut EntityPosition,
                &mut EntityVelocity,
                &mut EntityOrientation,
                &mut MovementIntent,
                &mut InteractionIntent,
                &mut EntityModel,
                &mut BoxCollider,
                &mut CollisionStatus,
            )>()
            .unwrap();

        let ray_origin = self.camera.get_eye_position(position.0);

        let base_properties = ModelDefinition::from_handle(model.model_id).properties();

        let mut mouse_ctx = MouseInputContext {
            chunks: &mut self.chunk_state,
            renderer: &mut self.renderer,
            resource_pack: &self.resource_pack,
            buttons: &self.pressed_mouse_buttons,
            client: &mut self.client,
            previous_state: &mut self.previous_state,
            orientation: &mut orientation,
            model: &mut model,
            base_properties: &base_properties,
            bounding_box: &mut collider,
            interact_intent: &mut mouse_intent,
        };

        Self::process_mouse_inputs(&mut mouse_ctx, ray_origin);

        let cursor_delta = self.camera.get_cursor_delta();

        orientation
            .0
            .yaw_pitch(
                (INVERT_YAW * cursor_delta.0 * MOUSE_SENSITIVITY) as f32,
                (INVERT_PITCH * cursor_delta.1 * MOUSE_SENSITIVITY) as f32,
            )
            .clamp(.., -1.5..1.5);

        *intent = MovementIntent {
            forward,
            strafe: right,
            fly: false,
            jump: self.pressed_keys.contains(&KeyCode::Space),
            sprint: self.pressed_keys.contains(&KeyCode::ControlLeft),
            sneak: self.pressed_keys.contains(&KeyCode::ShiftLeft),
        };

        let should_sync_intent = self.previous_state.intent != Some(*intent);
        // FIXME: quantize
        let should_sync_orientation = self.previous_state.orientation != Some(*orientation);

        if self.client.is_connected() {
            if should_sync_intent {
                let msg = ClientMessage::Move(*intent);
                msg.transmit(&mut self.client);
            }

            if should_sync_orientation {
                let msg = ClientMessage::Look(*orientation);
                msg.transmit(&mut self.client);
            }
        }

        if should_sync_intent {
            self.previous_state.intent = Some(*intent);
        }
        if should_sync_orientation {
            self.previous_state.orientation = Some(*orientation);
        }

        let new_velocity = ecs::movement::apply_gravity(
            velocity.0,
            base_properties.jump_velocity,
            &intent,
            *collision_status,
            dt,
        );

        let (new_position, new_velocity) = ecs::movement::apply_intent(
            position.0,
            orientation.0,
            &intent,
            base_properties.move_speed,
            new_velocity,
            dt,
        );

        let (final_position, final_velocity, new_status) = ecs::movement::apply_collision_aabb(
            new_position,
            *collider,
            *collision_status,
            new_velocity,
            &self.chunk_state,
            &self.resource_pack,
            dt,
        );

        *position = EntityPosition(final_position);
        *velocity = EntityVelocity(final_velocity);
        *collision_status = new_status;

        let bobbing_speed = if *collision_status == CollisionStatus::OnGround {
            Vec3fGlobal::new(intent.forward, 0.0, intent.strafe * 0.4).length()
                * if intent.sneak {
                    MovementIntent::SNEAK_MODIFIER
                } else if intent.sprint {
                    MovementIntent::SPRINT_MODIFIER
                } else {
                    1.0
                }
        } else {
            0.0
        };

        self.camera.update(bobbing_speed, dt);

        (position.0, orientation.0, final_velocity, base_properties)
    }

    fn process_mouse_inputs(context: &mut MouseInputContext<'_>, ray_origin: Vec3fGlobal) {
        let [left, right] = [MouseButton::Left, MouseButton::Right].map(|button| {
            let down = context.buttons.contains(&button);

            let just_pressed = if down {
                match context.previous_state.down.entry(button) {
                    Entry::Occupied(mut entry) => {
                        if entry.get().elapsed() < Duration::from_millis(200) {
                            false
                        } else {
                            entry.insert(Instant::now());
                            true
                        }
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(Instant::now());
                        true
                    }
                }
            } else {
                false
            };

            match button {
                MouseButton::Left => context.interact_intent.attack = just_pressed,
                MouseButton::Right => context.interact_intent.interact = just_pressed,
                _ => {}
            }

            just_pressed
        });

        let mut targeted_block = None;

        if left {
            let ray = raycast(
                ray_origin,
                context.orientation.0,
                context.base_properties.reach_distance,
                context.chunks,
                context.resource_pack,
            );

            if let Some((block, _, normal)) = ray {
                tracing::debug!(
                    "Client attacked block at {block:?} with normal {normal:?}, player position = {:?}, orientation = {:?}, eye height = {:?}",
                    ray_origin,
                    context.orientation.0,
                    context.model.eye_height,
                );

                targeted_block = Some((block.position(), normal));
                context
                    .chunks
                    .set_block(block.position().into(), BlockId::AIR);
                context
                    .chunks
                    .invalidate_chunk_meshes_around_block(context.renderer, block.position());
            }
        }

        // temporary
        if right {
            let entity_type = EntityType::ALL
                .get((*context.model.model_id + 1) as usize)
                .copied()
                .unwrap_or(EntityType::Human);
            let model_definition = entity_type.model();
            let entity_model = EntityModel::for_model(model_definition);

            let collider = spatial::aabb::BoxCollider::for_model(model_definition);

            context.model.model_id = entity_model.model_id;
            context.bounding_box.0 = collider;

            let msg = ClientMessage::RemodelEntity {
                model: entity_model,
                bounding_box: BoxCollider(collider),
            };
            msg.transmit(context.client);
        }

        if left || right {
            let msg = ClientMessage::InteractBlock {
                intent: *context.interact_intent,
                targeted_block,
            };
            msg.transmit(context.client);
        }
    }
}
