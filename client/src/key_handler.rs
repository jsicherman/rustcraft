use std::time::Duration;

use ecs::{
    BoxCollider, CollisionStatus, EntityOrientation, EntityPosition, EntityVelocity, MovementIntent,
};
use protocol::{CHANNEL_CHUNKS, ClientMessage, Packet};
use spatial::{orientation::Orientation, vectors::Vec3fGlobal};
use winit::keyboard::KeyCode;

use crate::{
    AppState,
    camera::{INVERT_PITCH, INVERT_YAW, MOUSE_SENSITIVITY},
};

impl AppState {
    pub(crate) fn process_inputs(&mut self, dt: Duration) -> (Vec3fGlobal, Orientation) {
        self.client.update(dt);
        self.transport.update(dt, &mut self.client).ok();

        let Some(player_network_id) = self.local_player_network_id else {
            return (Vec3fGlobal::new(0.0, 0.0, 0.0), Default::default());
        };

        let Some(&player_entity) = self.network_to_local.get(&player_network_id) else {
            return (Vec3fGlobal::new(0.0, 0.0, 0.0), Default::default());
        };

        let axis = |positive: KeyCode, negative: KeyCode| -> f32 {
            (self.pressed_keys.contains(&positive) as u8 as f32)
                - (self.pressed_keys.contains(&negative) as u8 as f32)
        };

        let forward = axis(KeyCode::KeyW, KeyCode::KeyS);
        let right = axis(KeyCode::KeyA, KeyCode::KeyD);
        let up = axis(KeyCode::Space, KeyCode::ShiftLeft);

        let mut entity = self.world.entity_mut(player_entity);

        let (
            mut position,
            mut velocity,
            mut orientation,
            mut intent,
            collider,
            mut collision_status,
        ) = entity
            .get_components_mut::<(
                &mut EntityPosition,
                &mut EntityVelocity,
                &mut EntityOrientation,
                &mut MovementIntent,
                &BoxCollider,
                &mut CollisionStatus,
            )>()
            .unwrap();

        let cursor_delta = self.camera.get_cursor_delta();

        orientation
            .0
            .yaw_pitch(
                (INVERT_YAW * cursor_delta.0 * MOUSE_SENSITIVITY) as f32,
                (INVERT_PITCH * cursor_delta.1 * MOUSE_SENSITIVITY) as f32,
            )
            .clamp(.., -1.5..1.5);

        *intent = MovementIntent::new(
            forward,
            right,
            up > 0.0,
            self.pressed_keys.contains(&KeyCode::ControlLeft),
            false,
            false,
        );

        let should_sync_intent = self.last_sent_intent != Some(*intent);
        // FIXME: quantize
        let should_sync_orientation = self.last_sent_orientation != Some(*orientation);

        if self.client.is_connected() {
            if should_sync_intent {
                let msg = ClientMessage::Move(*intent).encode().unwrap();
                self.client.send_message(CHANNEL_CHUNKS, msg);
            }

            if should_sync_orientation {
                let msg = ClientMessage::Look(*orientation).encode().unwrap();
                self.client.send_message(CHANNEL_CHUNKS, msg);
            }
        }

        if should_sync_intent {
            self.last_sent_intent = Some(*intent);
        }
        if should_sync_orientation {
            self.last_sent_orientation = Some(*orientation);
        }

        let new_velocity = ecs::movement::apply_gravity(velocity.0, &intent, *collision_status, dt);

        let (new_position, new_velocity) =
            ecs::movement::apply_intent(position.0, orientation.0, &intent, new_velocity, dt);

        let (final_position, final_velocity, new_status) = ecs::movement::apply_collision_aabb(
            new_position,
            *collider,
            *collision_status,
            new_velocity,
            &self.loaded_chunks,
            &self.block_registry,
            dt,
        );

        *position = EntityPosition(final_position);
        *velocity = EntityVelocity(final_velocity);
        *collision_status = new_status;

        (position.0, orientation.0)
    }
}
