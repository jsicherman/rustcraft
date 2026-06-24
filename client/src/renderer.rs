use std::collections::HashMap;

use ecs::Entity;
use render::model::RenderInstance;

use crate::chunk::ClientChunk;

#[derive(Default)]
pub struct RenderState {
    entity_to_instance: HashMap<Entity, RenderInstance>,
}

impl RenderState {
    pub fn iter(&self) -> impl Iterator<Item = (&Entity, &RenderInstance)> {
        self.entity_to_instance.iter()
    }
    pub fn num_instances(&self) -> usize {
        self.entity_to_instance.len()
    }

    pub fn set_instance(&mut self, entity: Entity, instance: RenderInstance) {
        self.entity_to_instance.insert(entity, instance);
    }

    pub fn remove_instance(&mut self, entity: &Entity) {
        self.entity_to_instance.remove(entity);
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
        self.instance.is_none() && self.queued
    }

    fn queued(&mut self, queued: bool) {
        self.queued = queued;
    }

    fn receive(&mut self, instance: RenderInstance) {
        self.instance = Some(instance);
        self.queued = false;
    }
}
