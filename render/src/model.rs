use std::{collections::HashMap, ops::Deref};

use serde::{Deserialize, Serialize};

use crate::{
    Mesh,
    math::{mat4_mul, transform_with_pivot},
    texture::MaterialTextures,
};

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
    pub(crate) material: Option<MaterialTextures>,
}

impl MeshAsset {
    pub fn vertex_count(&self) -> u32 {
        self.mesh.index_count()
    }
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

    /// Assign the same mesh to all leaf nodes in the tree
    pub fn with_geometry(mut self, mesh: MeshHandle) -> Self {
        Self::assign_meshes_recursive(&mut self.root, mesh);
        self
    }

    fn assign_meshes_recursive(node: &mut Node, mesh: MeshHandle) {
        if node.children.is_empty() {
            node.mesh = Some(mesh);
        } else {
            for child in &mut node.children {
                Self::assign_meshes_recursive(child, mesh);
            }
        }
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
    material_override: Option<u8>,

    children: Vec<Node>,

    transform: [[f32; 4]; 4],
    scale: [f32; 3],
}

impl Node {
    pub fn identity() -> Self {
        Self {
            name: None,
            mesh: None,
            material_override: None,
            children: Vec::new(),
            scale: [1.0, 1.0, 1.0],
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

    pub fn with_material(mut self, material: u8) -> Self {
        self.material_override = Some(material);
        self
    }

    pub fn with_child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }
}

#[derive(Debug)]
pub struct RenderInstance {
    pub(crate) handle: RenderHandle,
    pub(crate) transform: [[f32; 4]; 4],
    pub(crate) scale: [f32; 3],

    pub(crate) material_override: Option<MaterialTextures>,

    pub(crate) node_materials: HashMap<&'static str, MaterialTextures>,
    pub(crate) node_transforms: HashMap<&'static str, [[f32; 4]; 4]>,
    pub(crate) node_pivots: HashMap<&'static str, [f32; 3]>,
}

impl RenderInstance {
    pub fn new(handle: RenderHandle, transform: [[f32; 4]; 4], scale: [f32; 3]) -> Self {
        Self {
            handle,
            transform,
            scale,
            material_override: Default::default(),
            node_transforms: Default::default(),
            node_pivots: Default::default(),
            node_materials: Default::default(),
        }
    }

    pub fn with_transforms(
        mut self,
        nodes: impl IntoIterator<Item = (&'static str, [[f32; 4]; 4])>,
    ) -> Self {
        self.node_transforms = nodes.into_iter().collect();
        self
    }

    pub fn with_transforms_pivots(
        mut self,
        nodes: impl IntoIterator<Item = (&'static str, [[f32; 4]; 4])>,
        pivots: impl IntoIterator<Item = (&'static str, [f32; 3])>,
    ) -> Self {
        self.node_transforms = nodes.into_iter().collect();
        self.node_pivots = pivots.into_iter().collect();
        self
    }

    pub fn with_node_transform(mut self, node: &'static str, transform: [[f32; 4]; 4]) -> Self {
        self.node_transforms.insert(node, transform);
        self
    }

    pub fn with_node_pivot(mut self, node: &'static str, pivot: [f32; 3]) -> Self {
        self.node_pivots.insert(node, pivot);
        self
    }

    pub fn with_materials(
        mut self,
        nodes: impl IntoIterator<Item = (&'static str, MaterialTextures)>,
    ) -> Self {
        self.node_materials = nodes.into_iter().collect();
        self
    }

    pub fn with_node_material(mut self, node: &'static str, material: MaterialTextures) -> Self {
        self.node_materials.insert(node, material);
        self
    }

    pub fn with_material(mut self, material: MaterialTextures) -> Self {
        self.material_override = Some(material);
        self
    }

    pub fn handle(&self) -> RenderHandle {
        self.handle
    }

    pub fn translation(&self) -> [f32; 3] {
        [
            self.transform[3][0],
            self.transform[3][1],
            self.transform[3][2],
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderHandle {
    Mesh(MeshHandle),
    Model(ModelHandle),
}

pub struct RenderCommandGpu {
    pub(crate) mesh: MeshHandle,
    pub(crate) material: Option<MaterialTextures>,
    pub(crate) transform: [[f32; 4]; 4],
    pub(crate) scale: [f32; 3],
}

impl Asset for ModelAsset {
    fn build(&self, instance: &RenderInstance, stack: &mut Vec<RenderCommandGpu>) {
        fn texture_id_material(id: u8) -> MaterialTextures {
            [id as u32; 6]
        }

        fn _build(
            parent: &RenderInstance,
            node: &Node,
            parent_transform: [[f32; 4]; 4],
            stack: &mut Vec<RenderCommandGpu>,
        ) {
            let material = parent
                .node_materials
                .get(node.name.unwrap_or_default())
                .copied()
                .or(node.material_override.map(texture_id_material))
                .or(parent.material_override)
                .unwrap_or(ModelAsset::DEFAULT_TEXTURES);

            let composed = mat4_mul(node.transform, parent_transform);
            let transform = if let Some(local_transform) =
                node.name.and_then(|name| parent.node_transforms.get(name))
            {
                let adjusted_transform =
                    if let Some(&pivot) = node.name.and_then(|name| parent.node_pivots.get(name)) {
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
                    material: Some(material),
                    transform,
                    scale: node.scale,
                });
            }

            for child in &node.children {
                _build(parent, child, transform, stack);
            }
        }

        _build(instance, &self.root, instance.transform, stack);
    }
}

impl Asset for MeshAsset {
    fn build(&self, instance: &RenderInstance, stack: &mut Vec<RenderCommandGpu>) {
        let RenderHandle::Mesh(mesh) = instance.handle else {
            tracing::error!("Handle is not a mesh: {:?}", instance.handle);
            return;
        };

        let cmd = RenderCommandGpu {
            mesh,
            material: instance.material_override.or(self.material),
            transform: instance.transform,
            scale: instance.scale,
        };

        stack.push(cmd);
    }
}

pub trait Asset {
    const DEFAULT_TEXTURES: MaterialTextures = [0; 6];

    fn build(&self, instance: &RenderInstance, stack: &mut Vec<RenderCommandGpu>);
}
