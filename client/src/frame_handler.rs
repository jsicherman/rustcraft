use std::time::Duration;

use chunk::{Chunk, ChunkProvider};
use ecs::{Entity, EntityModel, EntityOrientation, EntityPosition};
use protocol::RENDER_DISTANCE_SQ;
use render::{
    DebugOverlayData,
    model::{RenderHandle, RenderInstance},
};
use spatial::{
    CHUNK_SIZE_SQ,
    orientation::Orientation,
    vectors::{VEC4F_IDENTITY, Vec2iChunk, Vec3fGlobal},
};

use crate::{AppState, renderer::NetworkRenderable};

impl AppState {
    pub fn request_entity_frames(&mut self, current_position: Vec3fGlobal) {
        let mut query = self
            .world
            .query::<(Entity, &EntityPosition, &EntityOrientation, &EntityModel)>();

        for (entity, position, orientation, model) in query.iter(&self.world) {
            if let Some((_, Some(local_entity))) = self.local_player
                && local_entity == entity
            {
                return;
            }

            if (position.0 - current_position).length_sq()
                > RENDER_DISTANCE_SQ as f32 * CHUNK_SIZE_SQ as f32
            {
                continue;
            }

            if !self.renderer.contains_model(&model.model()) {
                tracing::warn!("Missing model definition for handle {}", *model.model());
                continue;
            }

            let yaw = orientation.0 * [1.0, 0.0];
            let pitch = orientation.0 * [0.0, -1.0];

            self.entity_state.set_instance(
                entity,
                RenderInstance::new(
                    RenderHandle::Model(model.model()),
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
            );
        }
    }

    pub fn request_entity_frame(&mut self, entity: Entity, current_position: Vec3fGlobal) {
        if let Some((_, Some(local_entity))) = self.local_player
            && local_entity == entity
        {
            return;
        }

        let Ok((position, _orientation, model)) =
            self.world
                .entity(entity)
                .get_components::<(&EntityPosition, &EntityOrientation, &EntityModel)>()
        else {
            return;
        };

        if (position.0 - current_position).length_sq()
            > RENDER_DISTANCE_SQ as f32 * CHUNK_SIZE_SQ as f32
        {
            return;
        }

        if !self.renderer.contains_model(&model.model()) {
            tracing::warn!("Missing model definition for handle {}", *model.model());
            return;
        }

        self.entity_state.set_instance(
            entity,
            RenderInstance::new(
                RenderHandle::Model(model.model()),
                position
                    .0
                    .translation_matrix()
                    .map(std::convert::Into::into),
            ),
        );
    }

    pub fn request_chunk_frames(&mut self, current_chunk_position: Vec2iChunk) {
        let mut num_queued = 0;

        let voxels: Vec<_> = self
            .chunk_state
            .chunks
            .iter()
            .map(|(coordinate, client_chunk)| {
                if client_chunk.has_instance() || client_chunk.is_queued() {
                    return Default::default();
                }

                if (*coordinate - current_chunk_position).length_sq() > RENDER_DISTANCE_SQ {
                    return Default::default();
                }

                Some(
                    client_chunk
                        .iter(self.chunk_state.store())
                        .map(|b| b as u32)
                        .collect::<Vec<_>>(),
                )
            })
            .collect();

        for ((coordinate, client_chunk), voxels) in self.chunk_state.chunks.iter_mut().zip(voxels) {
            let Some(voxels) = voxels else {
                continue;
            };

            num_queued += 1;

            self.renderer.chunk_builder.enqueue(
                (coordinate.x(), coordinate.z()),
                voxels,
                Chunk::CHUNK_COLUMN,
            );
            client_chunk.queued(true);
        }

        if num_queued != 0 {
            tracing::debug!("Queued {num_queued} chunk frames");
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

        for chunk in self.chunk_state.chunks.values() {
            if let Some(instance) = chunk.instance() {
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
        }

        // FIXME: filter for distance?
        let (mut num_mesh, mut num_model) = (0, 0);
        for (_, instance) in self.entity_state.iter() {
            match instance.handle() {
                RenderHandle::Mesh(_) => num_mesh += 1,
                RenderHandle::Model(_) => num_model += 1,
            }

            instances.push(instance);
        }

        let size = self.window.inner_size();

        self.camera
            .set_aspect(size.width as f32 / size.height as f32);
        let vp = self
            .camera
            .view_projection(client_position, client_orientation);

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
        };

        self.renderer.render(
            &mut self.render_queue,
            &self.window,
            &instances,
            vp.map(std::convert::Into::into),
            &debug_overlay,
        );
    }
}
