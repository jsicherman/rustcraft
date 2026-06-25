use std::{collections::HashSet, time::Duration};

use block::{BlockId, REACH_DISTANCE, TexturePack};
use chunk::{ChunkProvider, raycasting::raycast};
use ecs::{
    BoxCollider, CollisionStatus, EntityModel, EntityOrientation, EntityPosition, EntityVelocity,
    InteractionIntent, MovementIntent,
};
use model::ModelDefinition;
use protocol::{CHANNEL_ENTITIES, ClientMessage, Packet};
use render::Renderer;
use renet::RenetClient;
use spatial::{
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
    textures: &'a TexturePack,
    buttons: &'a HashSet<MouseButton>,
    client: &'a mut RenetClient,
    previous_state: &'a mut PreviousState,
}

impl AppState {
    pub(crate) fn invalidate_chunk_meshes_around_block(&mut self, block_position: Vec3iGlobal) {
        self.chunk_state
            .invalidate_chunk_meshes_around_block(&mut self.renderer, block_position);
    }

    pub(crate) fn process_inputs(
        &mut self,
        dt: Duration,
    ) -> (Vec3fGlobal, Orientation, Vec3fGlobal) {
        self.client.update(dt);

        match self.transport.update(dt, &mut self.client) {
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Transport error: {e}");
            }
        }

        let Some((_, Some(player_entity))) = self.local_player else {
            return (Default::default(), Default::default(), Default::default());
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

        let mut mouse_ctx = MouseInputContext {
            chunks: &mut self.chunk_state,
            renderer: &mut self.renderer,
            textures: &self.texture_pack,
            buttons: &self.pressed_mouse_buttons,
            client: &mut self.client,
            previous_state: &mut self.previous_state,
        };

        Self::process_mouse_inputs(
            &mut mouse_ctx,
            ray_origin,
            &mut position,
            &mut orientation,
            &mut model,
            &mut collider,
            &mut mouse_intent,
        );

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
                let msg = ClientMessage::EntityMove(*intent).encode().unwrap();
                self.client.send_message(CHANNEL_ENTITIES, msg);
            }

            if should_sync_orientation {
                let msg = ClientMessage::EntityLook(*orientation).encode().unwrap();
                self.client.send_message(CHANNEL_ENTITIES, msg);
            }
        }

        if should_sync_intent {
            self.previous_state.intent = Some(*intent);
        }
        if should_sync_orientation {
            self.previous_state.orientation = Some(*orientation);
        }

        let new_velocity = ecs::movement::apply_gravity(velocity.0, &intent, *collision_status, dt);

        let (new_position, new_velocity) =
            ecs::movement::apply_intent(position.0, orientation.0, &intent, new_velocity, dt);

        let (final_position, final_velocity, new_status) = ecs::movement::apply_collision_aabb(
            new_position,
            *collider,
            *collision_status,
            new_velocity,
            &self.chunk_state,
            &self.texture_pack,
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

        (position.0, orientation.0, final_velocity)
    }

    fn process_mouse_inputs(
        context: &mut MouseInputContext<'_>,
        ray_origin: Vec3fGlobal,
        _position: &mut EntityPosition,
        orientation: &mut EntityOrientation,
        model: &mut EntityModel,
        bounding_box: &mut BoxCollider,
        interact_intent: &mut InteractionIntent,
    ) {
        let [left, right] = [MouseButton::Left, MouseButton::Right].map(|button| {
            let down = context.buttons.contains(&button);
            let just_pressed = down && !context.previous_state.down.contains(&button);
            if down {
                context.previous_state.down.insert(button);
            } else {
                context.previous_state.down.remove(&button);
            }

            match button {
                MouseButton::Left => interact_intent.attack = just_pressed,
                MouseButton::Right => interact_intent.interact = just_pressed,
                _ => {}
            }

            just_pressed
        });

        let mut targeted_block = None;

        if left {
            let ray = raycast(
                ray_origin,
                orientation.0,
                REACH_DISTANCE,
                context.chunks,
                context.textures,
            );

            if let Some((block, normal)) = ray {
                tracing::debug!(
                    "Client attacked block at {block:?} with normal {normal:?}, player position = {:?}, orientation = {:?}, eye height = {:?}",
                    ray_origin,
                    orientation.0,
                    model.eye_height,
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
            let definition = ModelDefinition::iter()
                .nth((*model.model_id + 1) as usize)
                .unwrap_or(ModelDefinition::Humanoid);
            let entity_model = EntityModel::for_model(definition);
            let collider = spatial::aabb::BoxCollider::for_model(definition);

            model.model_id = entity_model.model_id;
            bounding_box.0 = collider;

            let msg = ClientMessage::EntityRemodel {
                model: entity_model,
                bounding_box: BoxCollider(collider),
            }
            .encode()
            .unwrap();

            context.client.send_message(CHANNEL_ENTITIES, msg);
        }

        if left || right {
            let msg = ClientMessage::BlockInteract {
                intent: *interact_intent,
                targeted_block,
            }
            .encode()
            .unwrap();
            context.client.send_message(CHANNEL_ENTITIES, msg);
        }
    }
}
