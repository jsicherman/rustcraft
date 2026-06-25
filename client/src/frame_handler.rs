use std::time::Duration;

use block::REACH_DISTANCE;
use chunk::Chunk;
use chunk::raycasting::raycast;
use ecs::{BoxCollider, Entity, EntityModel, EntityOrientation, EntityPosition};
use protocol::RENDER_DISTANCE_SQ;
use render::{
    DebugOverlayData,
    model::{RenderHandle, RenderInstance},
};
use smallvec::SmallVec;
use spatial::{
    CHUNK_SIZE, CHUNK_SIZE_SQ, WORLD_HEIGHT,
    orientation::Orientation,
    vectors::{VEC4F_IDENTITY, Vec2iChunk, Vec3fGlobal},
};

use crate::{
    AppState,
    renderer::{CullSphere, NetworkRenderable},
};

impl AppState {
    fn cull_sphere(position: Vec3fGlobal, collider: BoxCollider) -> CullSphere {
        let half_height = collider.0.height() * 0.5;
        let half_width = collider.0.half_width();
        let radius = (half_height * half_height + 2.0 * half_width * half_width).sqrt();
        let center = [position[0], position[1] + half_height, position[2]].into();

        CullSphere::new(center, radius)
    }

    pub fn request_entity_frames(&mut self, current_position: Vec3fGlobal) {
        let mut query = self.world.query::<(
            Entity,
            &EntityPosition,
            &EntityOrientation,
            &EntityModel,
            &BoxCollider,
        )>();

        for (entity, position, orientation, model, collider) in query.iter(&self.world) {
            if let Some((_, Some(local_entity))) = self.local_player
                && local_entity == entity
            {
                continue;
            }

            if (position.0 - current_position).length_sq()
                > RENDER_DISTANCE_SQ as f32 * CHUNK_SIZE_SQ as f32
            {
                continue;
            }

            if !self.renderer.contains_model(&model.model_id) {
                tracing::warn!("Missing model definition for handle {}", *model.model_id);
                continue;
            }

            let yaw = orientation.0 * [1.0, 0.0];
            let pitch = orientation.0 * [0.0, -1.0];

            self.entity_state.set_entity(
                entity,
                RenderInstance::new(
                    RenderHandle::Model(model.model_id),
                    yaw.apply_to(position.0.translation_matrix())
                        .map(std::convert::Into::into),
                )
                .with_transforms_pivots(
                    [(
                        "head",
                        pitch.apply_to(VEC4F_IDENTITY).map(std::convert::Into::into),
                    )],
                    [("head", [-0.5, 0.0, -0.5])],
                ),
                //.with_node_transform("left_arm", arm_rotation)
                //.with_node_pivot("left_arm", [0.0, -0.375, -0.125]),
                Self::cull_sphere(position.0, *collider),
            );
        }
    }

    pub fn request_entity_frame(&mut self, entity: Entity, current_position: Vec3fGlobal) {
        if let Some((_, Some(local_entity))) = self.local_player
            && local_entity == entity
        {
            return;
        }

        let Ok((position, _orientation, model, collider)) =
            self.world.entity(entity).get_components::<(
                &EntityPosition,
                &EntityOrientation,
                &EntityModel,
                &BoxCollider,
            )>()
        else {
            return;
        };

        if (position.0 - current_position).length_sq()
            > RENDER_DISTANCE_SQ as f32 * CHUNK_SIZE_SQ as f32
        {
            return;
        }

        if !self.renderer.contains_model(&model.model_id) {
            tracing::warn!("Missing model definition for handle {}", *model.model_id);
            return;
        }

        self.entity_state.set_entity(
            entity,
            RenderInstance::new(
                RenderHandle::Model(model.model_id),
                position
                    .0
                    .translation_matrix()
                    .map(std::convert::Into::into),
            ),
            Self::cull_sphere(position.0, *collider),
        );
    }

    pub fn request_chunk_frames(&mut self, current_chunk_position: Vec2iChunk) {
        const MAX_CHUNK_MESH_QUEUES_PER_FRAME: usize = 2;

        let mut queued_frames = SmallVec::<[_; MAX_CHUNK_MESH_QUEUES_PER_FRAME]>::new();
        {
            let (chunks, store) = self.chunk_state.chunks_and_store_mut();

            for (coordinate, client_chunk) in chunks.iter_mut() {
                if client_chunk.is_queued() {
                    continue;
                }

                if client_chunk.has_instance() && !client_chunk.is_dirty() {
                    continue;
                }

                if (*coordinate - current_chunk_position).length_sq() > RENDER_DISTANCE_SQ {
                    continue;
                }

                if queued_frames.len() >= MAX_CHUNK_MESH_QUEUES_PER_FRAME {
                    break;
                }

                let voxels = chunk::materialize(client_chunk.chunk(), store);
                queued_frames.push((*coordinate, voxels));
                client_chunk.queued(true);
            }
        }

        for (coordinate, voxels) in queued_frames {
            self.renderer.chunk_builder.enqueue(
                (coordinate.x(), coordinate.z()),
                voxels,
                Chunk::CHUNK_COLUMN,
            );
        }
    }

    pub fn receive_chunk_frames(&mut self, current_chunk_position: Vec2iChunk) {
        for result in self.renderer.chunk_builder.collect_results() {
            let coordinate = Vec2iChunk::from(result.key);

            let Some(client_chunk) = self.chunk_state.chunks.get_mut(&coordinate) else {
                continue;
            };

            if (coordinate - current_chunk_position).length_sq() > RENDER_DISTANCE_SQ {
                client_chunk.queued(false);
                tracing::debug!("Dropped chunk: {coordinate:?}");
                continue;
            }

            let handle = self.renderer.upload_mesh(result.mesh);

            if let Some(old_instance) = client_chunk.instance.take() {
                let RenderHandle::Mesh(old_mesh) = old_instance.handle() else {
                    unreachable!();
                };
                self.renderer.remove_mesh(&old_mesh);
            }

            client_chunk.receive(RenderInstance::new(
                RenderHandle::Mesh(handle),
                coordinate
                    .translation_matrix()
                    .map(std::convert::Into::into),
            ));
        }
    }

    pub fn render_frame(
        &mut self,
        client_position: Vec3fGlobal,
        client_orientation: Orientation,
        dt: Duration,
    ) {
        let mut instances = Vec::new();

        let mut num_chunk_instances = 0;
        let mut num_vertices = 0;

        let size = self.window.inner_size();

        self.camera
            .set_aspect(size.width as f32 / size.height as f32);
        let vp = self
            .camera
            .view_projection(client_position, client_orientation);
        let skybox_vp = self.camera.skybox_view_projection(client_orientation);
        let frustum = self.camera.frustum(client_position, client_orientation);

        let chunk_height = WORLD_HEIGHT as f32;
        let chunk_size = CHUNK_SIZE as f32;

        for (coordinate, chunk) in &self.chunk_state.chunks {
            let Some(instance) = chunk.instance() else {
                continue;
            };

            let min: Vec3fGlobal = [
                coordinate.x() as f32 * chunk_size,
                0.0,
                coordinate.z() as f32 * chunk_size,
            ]
            .into();
            let max = [min[0] + chunk_size, chunk_height, min[2] + chunk_size].into();

            if !frustum.intersects_aabb(min, max) {
                continue;
            }

            let RenderHandle::Mesh(handle) = instance.handle() else {
                unreachable!();
            };
            num_chunk_instances += 1;
            num_vertices += self
                .renderer
                .meshes
                .get(&handle)
                .map(|asset| asset.vertex_count())
                .unwrap_or_default();

            instances.push(instance);
        }

        let (mut num_mesh, mut num_model) = (0, 0);
        for (_, instance, cull) in self.entity_state.iter() {
            if !frustum.intersects_sphere(cull.center(), cull.radius()) {
                continue;
            }

            match instance.handle() {
                RenderHandle::Mesh(_) => num_mesh += 1,
                RenderHandle::Model(_) => num_model += 1,
            }

            instances.push(instance);
        }

        let target_position_normal = raycast(
            self.camera.get_eye_position(client_position),
            client_orientation,
            REACH_DISTANCE,
            &self.chunk_state,
            &self.texture_pack,
        )
        .map(|(block, normal)| {
            let position: [i32; 3] = block.position().into();
            (
                [position[0] as f32, position[1] as f32, position[2] as f32],
                [normal[0] as f32, normal[1] as f32, normal[2] as f32],
            )
        });

        let debug_overlay = DebugOverlayData {
            player_pos: client_position.into(),
            yaw_radians: client_orientation.yaw(),
            pitch_radians: client_orientation.pitch(),
            vertex_count: num_vertices,
            chunk_count: num_chunk_instances as u32,
            mesh_count: num_mesh,
            model_count: num_model,
            entity_count: self.entity_state.num_instances() as u32,
            frame_time_ms: dt.as_millis(),
            average_frame_time_ms: self.frame_timer.avg(),
            time_of_day: self.time_of_day.to_hours(),
        };

        let overlay_particles = self.particles.collect_overlay_particles(client_position);

        self.renderer.render(
            &mut self.render_queue,
            &self.window,
            &instances,
            (
                vp.map(std::convert::Into::into),
                skybox_vp.map(std::convert::Into::into),
            ),
            target_position_normal,
            overlay_particles,
            &debug_overlay,
            self.time_of_day,
        );
    }
}
