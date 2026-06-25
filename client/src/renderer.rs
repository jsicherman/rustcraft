use std::collections::HashMap;

use ecs::Entity;
use render::model::RenderInstance;
use spatial::vectors::Vec3fGlobal;

use crate::chunk::ClientChunk;

#[derive(Clone, Copy, Debug)]
pub struct CullSphere {
    center: Vec3fGlobal,
    radius: f32,
}

impl CullSphere {
    pub fn new(center: Vec3fGlobal, radius: f32) -> Self {
        Self { center, radius }
    }

    pub fn center(&self) -> Vec3fGlobal {
        self.center
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }
}

struct EntityRenderData {
    instance: RenderInstance,
    cull: CullSphere,
}

#[derive(Default)]
pub struct RenderState {
    entity_to_render: HashMap<Entity, EntityRenderData>,
}

impl RenderState {
    pub fn iter(&self) -> impl Iterator<Item = (&Entity, &RenderInstance, &CullSphere)> {
        self.entity_to_render
            .iter()
            .map(|(entity, data)| (entity, &data.instance, &data.cull))
    }

    pub fn num_instances(&self) -> usize {
        self.entity_to_render.len()
    }

    pub fn set_entity(&mut self, entity: Entity, instance: RenderInstance, cull: CullSphere) {
        self.entity_to_render
            .insert(entity, EntityRenderData { instance, cull });
    }

    pub fn remove_instance(&mut self, entity: &Entity) {
        self.entity_to_render.remove(entity);
    }
}

pub trait NetworkRenderable {
    fn instance(&self) -> Option<&RenderInstance>;
    fn is_queued(&self) -> bool;
    fn queued(&mut self, queued: bool);
    fn receive(&mut self, instance: RenderInstance);

    fn has_instance(&self) -> bool {
        self.instance().is_some()
    }
}

impl NetworkRenderable for ClientChunk {
    fn instance(&self) -> Option<&RenderInstance> {
        self.instance.as_ref()
    }

    fn is_queued(&self) -> bool {
        self.queued
    }

    fn queued(&mut self, queued: bool) {
        self.queued = queued;
    }

    fn receive(&mut self, instance: RenderInstance) {
        self.instance = Some(instance);
        self.dirty = false;
        self.queued = false;
    }
}
