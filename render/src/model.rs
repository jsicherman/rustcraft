use std::{collections::HashMap, ops::Deref};

use block::TextureId;
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
    pub(crate) material: Option<MaterialTextures>,
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
    material_override: Option<TextureId>,
    children: Vec<Node>,
    transform: [[f32; 4]; 4],
}

fn scaled_translated(sx: f32, sy: f32, sz: f32, tx: f32, ty: f32, tz: f32) -> [[f32; 4]; 4] {
    [
        [sx, 0.0, 0.0, 0.0],
        [0.0, sy, 0.0, 0.0],
        [0.0, 0.0, sz, 0.0],
        [tx, ty, tz, 1.0],
    ]
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

    /// Textures: [head, torso, left_arm, right_arm, left_leg, right_leg]
    pub fn humanoid(textures: &[TextureId]) -> Self {
        assert_eq!(textures.len(), 6, "Humanoid model requires 6 textures");

        let head = Node::identity()
            .with_name("head")
            .with_material(textures[0])
            .with_transform(scaled_translated(0.5, 0.5, 0.5, -0.25, 1.30, -0.25));

        let torso = Node::identity()
            .with_name("torso")
            .with_material(textures[1])
            .with_transform(scaled_translated(0.5, 0.75, 0.25, -0.25, 0.55, -0.125));

        let left_arm = Node::identity()
            .with_name("left_arm")
            .with_material(textures[2])
            .with_transform(scaled_translated(0.25, 0.75, 0.25, -0.50, 0.55, -0.125));

        let right_arm = Node::identity()
            .with_name("right_arm")
            .with_material(textures[3])
            .with_transform(scaled_translated(0.25, 0.75, 0.25, 0.25, 0.55, -0.125));

        let left_leg = Node::identity()
            .with_name("left_leg")
            .with_material(textures[4])
            .with_transform(scaled_translated(0.25, 0.75, 0.25, -0.25, -0.20, -0.125));

        let right_leg = Node::identity()
            .with_name("right_leg")
            .with_material(textures[5])
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

    /// Textures: [head, snout, body, tail, front_left_leg, front_right_leg,
    /// back_left_leg, back_right_leg]
    pub fn quadruped(textures: &[TextureId]) -> Self {
        assert_eq!(textures.len(), 8, "Quadruped model requires 8 textures");

        let head = Node::identity()
            .with_name("head")
            .with_material(textures[0])
            .with_transform(scaled_translated(0.4, 0.35, 0.4, 0.25, 0.75, -0.20));

        let snout = Node::identity()
            .with_name("snout")
            .with_material(textures[1])
            .with_transform(scaled_translated(0.20, 0.15, 0.25, 0.55, 0.80, -0.125));

        let body = Node::identity()
            .with_name("body")
            .with_material(textures[2])
            .with_transform(scaled_translated(0.75, 0.40, 0.35, -0.375, 0.40, -0.175));

        let tail = Node::identity()
            .with_name("tail")
            .with_material(textures[3])
            .with_transform(scaled_translated(0.12, 0.35, 0.12, -0.44, 0.45, -0.06));

        let front_left_leg = Node::identity()
            .with_name("front_left_leg")
            .with_material(textures[4])
            .with_transform(scaled_translated(0.15, 0.45, 0.15, 0.10, -0.05, -0.075));

        let front_right_leg = Node::identity()
            .with_name("front_right_leg")
            .with_material(textures[5])
            .with_transform(scaled_translated(0.15, 0.45, 0.15, 0.28, -0.05, -0.075));

        let back_left_leg = Node::identity()
            .with_name("back_left_leg")
            .with_material(textures[6])
            .with_transform(scaled_translated(0.15, 0.45, 0.15, -0.28, -0.05, -0.075));

        let back_right_leg = Node::identity()
            .with_name("back_right_leg")
            .with_material(textures[7])
            .with_transform(scaled_translated(0.15, 0.45, 0.15, -0.10, -0.05, -0.075));

        Self::with_root(
            Node::identity()
                .with_child(head)
                .with_child(snout)
                .with_child(body)
                .with_child(tail)
                .with_child(front_left_leg)
                .with_child(front_right_leg)
                .with_child(back_left_leg)
                .with_child(back_right_leg),
        )
    }

    /// Textures: [head, ...segments (n >= 1), tail]
    pub fn snake(textures: &[TextureId]) -> Self {
        assert!(
            textures.len() >= 3,
            "Snake model requires at least 3 textures (head, 1+ segments, tail)"
        );

        let segment_count = textures.len() - 2;

        let head = Node::identity()
            .with_name("head")
            .with_material(textures[0])
            .with_transform(scaled_translated(0.30, 0.25, 0.30, -0.15, 0.05, -0.15));

        let mut root = Node::identity().with_child(head);

        for i in 0..segment_count {
            let t = i as f32 / segment_count.max(1) as f32;
            let s = 0.25 - t * 0.13;
            let offset_x = (i + 1) as f32 * -(s + 0.02);
            let segment = Node::identity()
                .with_name(Box::leak(format!("segment_{i}").into_boxed_str()))
                .with_material(textures[1 + i])
                .with_transform(scaled_translated(s, s * 0.9, s, offset_x, 0.05, -s * 0.5));
            root = root.with_child(segment);
        }

        let tail_s = 0.10;
        let tail_offset_x = -((segment_count + 1) as f32) * 0.20;
        let tail = Node::identity()
            .with_name("tail")
            .with_material(*textures.last().unwrap())
            .with_transform(scaled_translated(
                tail_s,
                tail_s * 0.8,
                tail_s,
                tail_offset_x,
                0.05,
                -tail_s * 0.5,
            ));

        Self::with_root(root.with_child(tail))
    }

    /// Textures: [head, beak, body, left_wing, right_wing, tail, left_leg, right_leg]
    pub fn bird(textures: &[TextureId]) -> Self {
        assert_eq!(textures.len(), 8, "Bird model requires 8 textures");

        let head = Node::identity()
            .with_name("head")
            .with_material(textures[0])
            .with_transform(scaled_translated(0.30, 0.28, 0.28, 0.10, 0.80, -0.14));

        let beak = Node::identity()
            .with_name("beak")
            .with_material(textures[1])
            .with_transform(scaled_translated(0.18, 0.08, 0.10, 0.35, 0.86, -0.05));

        let body = Node::identity()
            .with_name("body")
            .with_material(textures[2])
            .with_transform(scaled_translated(0.40, 0.35, 0.30, -0.20, 0.45, -0.15));

        let left_wing = Node::identity()
            .with_name("left_wing")
            .with_material(textures[3])
            .with_transform(scaled_translated(0.45, 0.10, 0.50, -0.225, 0.60, -0.65));

        let right_wing = Node::identity()
            .with_name("right_wing")
            .with_material(textures[4])
            .with_transform(scaled_translated(0.45, 0.10, 0.50, -0.225, 0.60, 0.15));

        let tail = Node::identity()
            .with_name("tail")
            .with_material(textures[5])
            .with_transform(scaled_translated(0.20, 0.22, 0.10, -0.35, 0.50, -0.05));

        let left_leg = Node::identity()
            .with_name("left_leg")
            .with_material(textures[6])
            .with_transform(scaled_translated(0.08, 0.28, 0.08, -0.04, -0.02, -0.18));

        let right_leg = Node::identity()
            .with_name("right_leg")
            .with_material(textures[7])
            .with_transform(scaled_translated(0.08, 0.28, 0.08, 0.06, -0.02, -0.08));

        Self::with_root(
            Node::identity()
                .with_child(head)
                .with_child(beak)
                .with_child(body)
                .with_child(left_wing)
                .with_child(right_wing)
                .with_child(tail)
                .with_child(left_leg)
                .with_child(right_leg),
        )
    }

    /// Textures: [head, fangs, thorax, abdomen, leg_pair_0, leg_pair_1, leg_pair_2, leg_pair_3, spinnerets]
    pub fn spider(textures: &[TextureId]) -> Self {
        assert_eq!(textures.len(), 9, "Spider model requires 9 textures");

        let head = Node::identity()
            .with_name("head")
            .with_material(textures[0])
            .with_transform(scaled_translated(0.28, 0.22, 0.28, 0.22, 0.28, -0.14));

        let fangs = Node::identity()
            .with_name("fangs")
            .with_material(textures[1])
            .with_transform(scaled_translated(0.10, 0.16, 0.12, 0.45, 0.20, -0.08));

        let thorax = Node::identity()
            .with_name("thorax")
            .with_material(textures[2])
            .with_transform(scaled_translated(0.35, 0.28, 0.35, -0.175, 0.22, -0.175));

        let abdomen = Node::identity()
            .with_name("abdomen")
            .with_material(textures[3])
            .with_transform(scaled_translated(0.40, 0.35, 0.40, -0.52, 0.18, -0.20));

        let spinnerets = Node::identity()
            .with_name("spinnerets")
            .with_material(textures[8])
            .with_transform(scaled_translated(0.10, 0.10, 0.10, -0.68, 0.26, -0.05));

        let leg_offsets: [(f32, f32); 4] =
            [(0.06, 0.40), (-0.04, 0.28), (-0.14, 0.18), (-0.24, 0.12)];

        let mut root = Node::identity()
            .with_child(head)
            .with_child(fangs)
            .with_child(thorax)
            .with_child(abdomen)
            .with_child(spinnerets);

        for (i, (ox, oy)) in leg_offsets.iter().enumerate() {
            let left = Node::identity()
                .with_name(Box::leak(format!("leg_left_{i}").into_boxed_str()))
                .with_material(textures[4 + i])
                .with_transform(scaled_translated(0.50, 0.06, 0.06, ox - 0.03, *oy, -0.50));

            let right = Node::identity()
                .with_name(Box::leak(format!("leg_right_{i}").into_boxed_str()))
                .with_material(textures[4 + i])
                .with_transform(scaled_translated(0.50, 0.06, 0.06, ox - 0.03, *oy, 0.14));

            root = root.with_child(left).with_child(right);
        }

        Self::with_root(root)
    }

    /// Textures: [body_top, body_mid, body_bot, eye_left, eye_right, drip_left, drip_right]
    pub fn slime(textures: &[TextureId]) -> Self {
        assert_eq!(textures.len(), 7, "Slime model requires 7 textures");

        // Three stacked tiers, widening toward the base
        let body_top = Node::identity()
            .with_name("body_top")
            .with_material(textures[0])
            .with_transform(scaled_translated(0.55, 0.28, 0.55, -0.275, 0.70, -0.275));

        let body_mid = Node::identity()
            .with_name("body_mid")
            .with_material(textures[1])
            .with_transform(scaled_translated(0.70, 0.32, 0.70, -0.35, 0.38, -0.35));

        let body_bot = Node::identity()
            .with_name("body_bot")
            .with_material(textures[2])
            .with_transform(scaled_translated(0.85, 0.42, 0.85, -0.425, 0.0, -0.425));

        let eye_left = Node::identity()
            .with_name("eye_left")
            .with_material(textures[3])
            .with_transform(scaled_translated(0.12, 0.14, 0.10, -0.10, 0.84, -0.30));

        let eye_right = Node::identity()
            .with_name("eye_right")
            .with_material(textures[4])
            .with_transform(scaled_translated(0.12, 0.14, 0.10, 0.08, 0.84, -0.30));

        let drip_left = Node::identity()
            .with_name("drip_left")
            .with_material(textures[5])
            .with_transform(scaled_translated(0.12, 0.10, 0.12, -0.22, -0.02, -0.28));

        let drip_right = Node::identity()
            .with_name("drip_right")
            .with_material(textures[6])
            .with_transform(scaled_translated(0.12, 0.10, 0.12, 0.04, -0.02, -0.28));

        Self::with_root(
            Node::identity()
                .with_child(body_top)
                .with_child(body_mid)
                .with_child(body_bot)
                .with_child(eye_left)
                .with_child(eye_right)
                .with_child(drip_left)
                .with_child(drip_right),
        )
    }

    /// Textures: [head, horn_left, horn_right, torso, shoulder_left,
    /// shoulder_right, left_arm, right_arm, left_leg, right_leg]
    pub fn giant(textures: &[TextureId]) -> Self {
        assert_eq!(textures.len(), 10, "Giant model requires 10 textures");

        let head = Node::identity()
            .with_name("head")
            .with_material(textures[0])
            .with_transform(scaled_translated(1.40, 1.30, 1.40, -0.70, 3.90, -0.70));

        let horn_left = Node::identity()
            .with_name("horn_left")
            .with_material(textures[1])
            .with_transform(scaled_translated(0.30, 0.80, 0.30, -0.70, 5.00, -0.15));

        let horn_right = Node::identity()
            .with_name("horn_right")
            .with_material(textures[2])
            .with_transform(scaled_translated(0.30, 0.80, 0.30, 0.30, 5.00, -0.15));

        let torso = Node::identity()
            .with_name("torso")
            .with_material(textures[3])
            .with_transform(scaled_translated(1.60, 2.00, 0.80, -0.80, 1.70, -0.40));

        let shoulder_left = Node::identity()
            .with_name("shoulder_left")
            .with_material(textures[4])
            .with_transform(scaled_translated(0.60, 0.55, 0.60, -1.55, 3.30, -0.30));

        let shoulder_right = Node::identity()
            .with_name("shoulder_right")
            .with_material(textures[5])
            .with_transform(scaled_translated(0.60, 0.55, 0.60, 0.95, 3.30, -0.30));

        let left_arm = Node::identity()
            .with_name("left_arm")
            .with_material(textures[6])
            .with_transform(scaled_translated(0.70, 2.00, 0.70, -1.55, 1.60, -0.35));

        let right_arm = Node::identity()
            .with_name("right_arm")
            .with_material(textures[7])
            .with_transform(scaled_translated(0.70, 2.00, 0.70, 0.85, 1.60, -0.35));

        let left_leg = Node::identity()
            .with_name("left_leg")
            .with_material(textures[8])
            .with_transform(scaled_translated(0.70, 1.80, 0.70, -0.75, -0.10, -0.35));

        let right_leg = Node::identity()
            .with_name("right_leg")
            .with_material(textures[9])
            .with_transform(scaled_translated(0.70, 1.80, 0.70, 0.05, -0.10, -0.35));

        Self::with_root(
            Node::identity()
                .with_child(head)
                .with_child(horn_left)
                .with_child(horn_right)
                .with_child(torso)
                .with_child(shoulder_left)
                .with_child(shoulder_right)
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

    pub fn with_material(mut self, material: TextureId) -> Self {
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
    pub(crate) material_override: Option<MaterialTextures>,
    pub(crate) node_materials: HashMap<&'static str, MaterialTextures>,
    pub(crate) node_transforms: HashMap<&'static str, [[f32; 4]; 4]>,
    pub(crate) node_pivots: HashMap<&'static str, [f32; 3]>,
}

impl RenderInstance {
    pub fn new(handle: RenderHandle, transform: [[f32; 4]; 4]) -> Self {
        Self {
            handle,
            transform,
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
}

impl Asset for ModelAsset {
    fn build(&self, instance: &RenderInstance, stack: &mut Vec<RenderCommandGpu>) {
        fn texture_id_material(id: TextureId) -> MaterialTextures {
            MaterialTextures([*id as u32; 6])
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
                });
            }

            for child in &node.children {
                _build(parent, child, transform, stack);
            }
        }

        _build(instance, &self.root, instance.transform, stack);
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

fn transform_with_pivot(local_transform: [[f32; 4]; 4], pivot: [f32; 3]) -> [[f32; 4]; 4] {
    // To rotate around a pivot point: translate(pivot) * transform * translate(-pivot)
    let translate_to_pivot = translate(pivot[0], pivot[1], pivot[2]);
    let translate_from_pivot = translate(-pivot[0], -pivot[1], -pivot[2]);
    mat4_mul(
        translate_to_pivot,
        mat4_mul(local_transform, translate_from_pivot),
    )
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
            material: instance.material_override.or(self.material),
            transform: instance.transform,
        });
    }
}

pub trait Asset {
    const DEFAULT_TEXTURES: MaterialTextures = MaterialTextures([0; 6]);

    fn build(&self, instance: &RenderInstance, stack: &mut Vec<RenderCommandGpu>);
}
