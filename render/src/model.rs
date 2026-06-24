use std::{collections::HashMap, ops::Deref};

use serde::{Deserialize, Serialize};

use crate::{Mesh, texture::MaterialTextures};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MeshHandle(u32);
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelHandle(u32);

impl From<u32> for MeshHandle {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl Deref for MeshHandle {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u32> for ModelHandle {
    fn from(value: u32) -> Self {
        Self(value)
    }
}
impl Deref for ModelHandle {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct MeshAsset {
    pub(crate) mesh: Mesh,
    pub(crate) material: MaterialTextures,
}

impl MeshAsset {
    pub fn vertex_count(&self) -> u32 {
        self.mesh.index_count()
    }
}

#[derive(Debug, Clone)]
pub struct ModelAsset {
    root: Node,
}

#[derive(Debug, Clone)]
pub struct Node {
    name: Option<&'static str>,
    mesh: Option<MeshHandle>,
    material_override: Option<MaterialTextures>,
    children: Vec<Node>,
    transform: [[f32; 4]; 4],
}

impl ModelAsset {
    pub fn empty() -> Self {
        Self {
            root: Node::identity(),
        }
    }

    pub fn with_root(root: Node) -> Self {
        Self { root }
    }

    pub fn humanoid() -> Self {
        fn scaled_translated(
            sx: f32,
            sy: f32,
            sz: f32,
            tx: f32,
            ty: f32,
            tz: f32,
        ) -> [[f32; 4]; 4] {
            [
                [sx, 0.0, 0.0, 0.0],
                [0.0, sy, 0.0, 0.0],
                [0.0, 0.0, sz, 0.0],
                [tx, ty, tz, 1.0],
            ]
        }

        // Feet are at y=0.0, total height is 1.8 units.
        let head = Node::identity()
            .with_name("head")
            .with_transform(scaled_translated(0.5, 0.5, 0.5, -0.25, 1.30, -0.25));

        let torso = Node::identity()
            .with_name("torso")
            .with_transform(scaled_translated(0.5, 0.75, 0.25, -0.25, 0.55, -0.125));

        let left_arm = Node::identity()
            .with_name("left_arm")
            .with_transform(scaled_translated(0.25, 0.75, 0.25, -0.50, 0.55, -0.125));

        let right_arm = Node::identity()
            .with_name("right_arm")
            .with_transform(scaled_translated(0.25, 0.75, 0.25, 0.25, 0.55, -0.125));

        let left_leg = Node::identity()
            .with_name("left_leg")
            .with_transform(scaled_translated(0.25, 0.75, 0.25, -0.25, -0.20, -0.125));

        let right_leg = Node::identity()
            .with_name("right_leg")
            .with_transform(scaled_translated(0.25, 0.75, 0.25, 0.00, -0.20, -0.125));

        Self::with_root(
            Node::identity()
                .with_child(head)
                .with_child(torso)
                .with_child(left_arm)
                .with_child(right_arm)
                .with_child(left_leg)
                .with_child(right_leg),
        )
    }

    /// Assign the same mesh to all leaf nodes in the tree
    pub fn with_geometry(mut self, mesh: MeshHandle) -> Self {
        Self::assign_meshes_recursive(&mut self.root, mesh);
        self
    }

    fn assign_meshes_recursive(node: &mut Node, mesh: MeshHandle) {
        if node.children.is_empty() {
            // Leaf node: assign mesh
            node.mesh = Some(mesh);
        } else {
            for child in &mut node.children {
                Self::assign_meshes_recursive(child, mesh);
            }
        }
    }
}

impl Node {
    pub fn identity() -> Self {
        Self {
            name: None,
            mesh: None,
            material_override: None,
            children: Vec::new(),
            transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    pub fn with_mesh(mesh: MeshHandle) -> Self {
        let mut node = Self::identity();
        node.mesh = Some(mesh);
        node
    }

    pub fn with_transform(mut self, transform: [[f32; 4]; 4]) -> Self {
        self.transform = transform;
        self
    }

    pub fn with_material_override(mut self, material: MaterialTextures) -> Self {
        self.material_override = Some(material);
        self
    }

    pub fn with_child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }
}

pub struct RenderInstance {
    pub(crate) handle: RenderHandle,
    pub(crate) transform: [[f32; 4]; 4],
    pub(crate) material_override: Option<MaterialTextures>,
    pub(crate) node_transforms: HashMap<&'static str, [[f32; 4]; 4]>,
    pub(crate) node_pivots: HashMap<&'static str, [f32; 3]>,
}

impl RenderInstance {
    pub fn new(handle: RenderHandle, transform: [[f32; 4]; 4]) -> Self {
        Self {
            handle,
            transform,
            node_transforms: HashMap::new(),
            node_pivots: HashMap::new(),
            material_override: None,
        }
    }

    pub fn with_node_transform(mut self, node: &'static str, transform: [[f32; 4]; 4]) -> Self {
        self.node_transforms.insert(node, transform);
        self
    }

    pub fn with_node_pivot(mut self, node: &'static str, pivot: [f32; 3]) -> Self {
        self.node_pivots.insert(node, pivot);
        self
    }

    pub fn with_material_override(mut self, material: MaterialTextures) -> Self {
        self.material_override = Some(material);
        self
    }

    pub fn handle(&self) -> RenderHandle {
        self.handle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderHandle {
    Mesh(MeshHandle),
    Model(ModelHandle),
}

pub struct RenderCommandGpu {
    pub(crate) mesh: MeshHandle,
    pub(crate) material: MaterialTextures,
    pub(crate) transform: [[f32; 4]; 4],
}

impl Asset for ModelAsset {
    fn build(&self, instance: &RenderInstance, stack: &mut Vec<RenderCommandGpu>) {
        fn _build(
            parent: &RenderInstance,
            node: &Node,
            parent_transform: [[f32; 4]; 4],
            parent_material: Option<&MaterialTextures>,
            stack: &mut Vec<RenderCommandGpu>,
        ) {
            let material = node
                .material_override
                .as_ref()
                .or(parent_material)
                .copied()
                .unwrap_or(ModelAsset::DEFAULT_TEXTURES);

            let composed = mat4_mul(node.transform, parent_transform);
            let transform = if let Some(local_transform) =
                node.name.and_then(|name| parent.node_transforms.get(name))
            {
                let adjusted_transform = if let Some(&pivot) =
                    node.name.and_then(|name| parent.node_pivots.get(name))
                {
                    transform_with_pivot(*local_transform, pivot)
                } else {
                    *local_transform
                };
                mat4_mul(adjusted_transform, composed)
            } else {
                composed
            };

            if let Some(mesh) = node.mesh {
                stack.push(RenderCommandGpu {
                    mesh,
                    material,
                    transform,
                });
            }

            for child in &node.children {
                _build(parent, child, transform, Some(&material), stack);
            }
        }

        _build(
            instance,
            &self.root,
            instance.transform,
            instance.material_override.as_ref(),
            stack,
        );
    }
}

fn translate(x: f32, y: f32, z: f32) -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x, y, z, 1.0],
    ]
}

fn transform_with_pivot(
    local_transform: [[f32; 4]; 4],
    pivot: [f32; 3],
) -> [[f32; 4]; 4] {
    // To rotate around a pivot point: translate(pivot) * transform * translate(-pivot)
    let translate_to_pivot = translate(pivot[0], pivot[1], pivot[2]);
    let translate_from_pivot = translate(-pivot[0], -pivot[1], -pivot[2]);
    mat4_mul(translate_to_pivot, mat4_mul(local_transform, translate_from_pivot))
}

fn mat4_mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                out[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    out
}

impl Asset for MeshAsset {
    fn build(&self, instance: &RenderInstance, stack: &mut Vec<RenderCommandGpu>) {
        let RenderHandle::Mesh(mesh) = instance.handle else {
            tracing::error!("Handle is not a mesh: {:?}", instance.handle);
            return;
        };

        stack.push(RenderCommandGpu {
            mesh,
            material: instance.material_override.unwrap_or(self.material),
            transform: instance.transform,
        });
    }
}

pub trait Asset {
    const DEFAULT_TEXTURES: MaterialTextures = MaterialTextures([0; 6]);

    fn build(&self, instance: &RenderInstance, stack: &mut Vec<RenderCommandGpu>);
}
