use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::vectors::{Chunk, CoordSpace, Global, IntoSpace, Vec2iChunk, Vec3f};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxCollider {
    height: f32,
    half_width: f32,
}

impl Default for BoxCollider {
    fn default() -> Self {
        Self {
            height: 1.8,
            half_width: 0.4,
        }
    }
}

impl Aabb for BoxCollider {
    fn aabb<S: CoordSpace>(&self, position: Vec3f<S>) -> AxisAlignedBoundingBox<S> {
        let min = Vec3f::from([
            position.x() - self.half_width,
            position.y(),
            position.z() - self.half_width,
        ]);
        let max = Vec3f::from([
            position.x() + self.half_width,
            position.y() + self.height,
            position.z() + self.half_width,
        ]);

        AxisAlignedBoundingBox::new(min, max)
    }

    fn aabb_swept<S: CoordSpace>(
        &self,
        position: Vec3f<S>,
        velocity: Vec3f<S>,
        dt: Duration,
    ) -> AxisAlignedBoundingBox<S> {
        let min = Vec3f::from([
            position.x() - self.half_width + velocity.x().min(0.0) * dt.as_secs_f32(),
            position.y() + (velocity.y().min(0.0) * dt.as_secs_f32()),
            position.z() - self.half_width + velocity.z().min(0.0) * dt.as_secs_f32(),
        ]);
        let max = Vec3f::from([
            position.x() + self.half_width + velocity.x().max(0.0) * dt.as_secs_f32(),
            position.y() + self.height + (velocity.y().max(0.0) * dt.as_secs_f32()),
            position.z() + self.half_width + velocity.z().max(0.0) * dt.as_secs_f32(),
        ]);

        AxisAlignedBoundingBox::new(min, max)
    }
}

pub trait Aabb {
    fn aabb<S: CoordSpace>(&self, position: Vec3f<S>) -> AxisAlignedBoundingBox<S>;
    fn aabb_swept<S: CoordSpace>(
        &self,
        position: Vec3f<S>,
        velocity: Vec3f<S>,
        dt: Duration,
    ) -> AxisAlignedBoundingBox<S>;
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AxisAlignedBoundingBox<S: CoordSpace> {
    min: Vec3f<S>,
    max: Vec3f<S>,
}

impl<S: CoordSpace> AxisAlignedBoundingBox<S> {
    pub fn new(min: Vec3f<S>, max: Vec3f<S>) -> Self {
        Self { min, max }
    }

    pub fn min(&self) -> Vec3f<S> {
        self.min
    }

    pub fn max(&self) -> Vec3f<S> {
        self.max
    }

    pub fn intersects_epsilon(&self, other: &Self, epsilon: f32) -> bool {
        self.max().x() + epsilon > other.min().x()
            && self.min().x() - epsilon < other.max().x()
            && self.max().y() + epsilon > other.min().y()
            && self.min().y() - epsilon < other.max().y()
            && self.max().z() + epsilon > other.min().z()
            && self.min().z() - epsilon < other.max().z()
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.max().x() > other.min().x()
            && self.min().x() < other.max().x()
            && self.max().y() > other.min().y()
            && self.min().y() < other.max().y()
            && self.max().z() > other.min().z()
            && self.min().z() < other.max().z()
    }

    pub fn intersects_overlaps(&self, other: &Self) -> bool {
        self.max().x() >= other.min().x()
            && self.min().x() <= other.max().x()
            && self.max().y() >= other.min().y()
            && self.min().y() <= other.max().y()
            && self.max().z() >= other.min().z()
            && self.min().z() <= other.max().z()
    }
}

impl AxisAlignedBoundingBox<Global> {
    pub fn chunks(&self) -> impl Iterator<Item = Vec2iChunk> {
        let min_chunk = IntoSpace::<Chunk>::into_space(self.min().floor());
        let max_chunk = IntoSpace::<Chunk>::into_space(self.max().ceil());

        let min_x = min_chunk.x() as i32;
        let max_x = max_chunk.x().ceil() as i32;
        let min_z = min_chunk.z() as i32;
        let max_z = max_chunk.z().ceil() as i32;

        (min_x..=max_x).flat_map(move |x| (min_z..=max_z).map(move |z| Vec2iChunk::new(x, z)))
    }
}
