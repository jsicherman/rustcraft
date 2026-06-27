use render::model::{MeshHandle, ModelAsset, ModelHandle, Node};

use crate::texture::TextureId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityProperties {
    pub reach_distance: f32,
    pub jump_velocity: f32,
    pub move_speed: f32,
}

impl Default for EntityProperties {
    fn default() -> Self {
        ModelDefinition::Humanoid.properties()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Human,
    Horse,
    Snake,
    Bird,
    Giant,
    Slime,
    Spider,
}

impl EntityType {
    pub const ALL: [Self; 7] = [
        Self::Human,
        Self::Horse,
        Self::Snake,
        Self::Bird,
        Self::Giant,
        Self::Slime,
        Self::Spider,
    ];

    pub fn handle(self) -> u32 {
        ModelDefinition::from(self) as u32
    }

    pub fn model(self) -> ModelDefinition {
        ModelDefinition::from(self)
    }

    pub fn textures(&self) -> &[TextureId] {
        match self {
            Self::Human => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(2),
                TextureId(2),
                TextureId(2),
            ],
            Self::Horse => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(2),
                TextureId(1),
            ],
            Self::Snake => &[TextureId(0), TextureId(1), TextureId(2), TextureId(1)],
            Self::Bird => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(0),
                TextureId(0),
                TextureId(1),
                TextureId(2),
            ],
            Self::Giant => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(2),
                TextureId(1),
                TextureId(3),
                TextureId(2),
            ],
            Self::Slime => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(3),
                TextureId(2),
                TextureId(1),
            ],
            Self::Spider => &[
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(0),
                TextureId(1),
                TextureId(2),
                TextureId(3),
                TextureId(2),
                TextureId(1),
            ],
        }
    }
}

impl From<EntityType> for ModelDefinition {
    fn from(entity_type: EntityType) -> Self {
        match entity_type {
            EntityType::Human => Self::Humanoid,
            EntityType::Horse => Self::Quadruped,
            EntityType::Snake => Self::Snake,
            EntityType::Bird => Self::Bird,
            EntityType::Giant => Self::Giant,
            EntityType::Slime => Self::Slime,
            EntityType::Spider => Self::Spider,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelDefinition {
    Humanoid = 0,
    Quadruped = 1,
    Snake = 2,
    Bird = 3,
    Giant = 4,
    Slime = 5,
    Spider = 6,
}

impl ModelDefinition {
    pub const ALL: [Self; 7] = [
        Self::Humanoid,
        Self::Quadruped,
        Self::Snake,
        Self::Bird,
        Self::Giant,
        Self::Slime,
        Self::Spider,
    ];

    pub fn from_handle(model: ModelHandle) -> Self {
        Self::ALL[*model as usize]
    }

    pub const fn properties(self) -> EntityProperties {
        match self {
            Self::Humanoid => EntityProperties {
                reach_distance: 3.0,
                move_speed: 4.2,
                jump_velocity: 6.3,
            },
            Self::Quadruped => EntityProperties {
                reach_distance: 2.5,
                move_speed: 3.8,
                jump_velocity: 5.0,
            },
            Self::Snake => EntityProperties {
                reach_distance: 1.5,
                move_speed: 2.5,
                jump_velocity: 3.0,
            },
            Self::Bird => EntityProperties {
                reach_distance: 2.0,
                move_speed: 5.0,
                jump_velocity: 4.0,
            },
            Self::Giant => EntityProperties {
                reach_distance: 6.0,
                move_speed: 2.0,
                jump_velocity: 3.5,
            },
            Self::Slime => EntityProperties {
                reach_distance: 1.0,
                move_speed: 1.5,
                jump_velocity: 2.0,
            },
            Self::Spider => EntityProperties {
                reach_distance: 1.5,
                move_speed: 2.5,
                jump_velocity: 3.0,
            },
        }
    }

    pub fn eye_height(self) -> f32 {
        match self {
            Self::Humanoid => 1.68,
            Self::Bird => 0.8,
            Self::Quadruped => 0.9,
            Self::Snake => 0.2,
            Self::Giant => 5.5,
            Self::Slime => 0.8,
            Self::Spider => 0.3,
        }
    }

    pub fn height(self) -> f32 {
        match self {
            Self::Humanoid => 1.8,
            Self::Bird => 1.1,
            Self::Quadruped => 1.15,
            Self::Snake => 0.3,
            Self::Giant => 5.9,
            Self::Slime => 1.0,
            Self::Spider => 0.4,
        }
    }

    pub fn half_width(self) -> f32 {
        match self {
            Self::Humanoid => 0.4,
            Self::Bird => 0.4,
            Self::Quadruped => 0.55,
            Self::Snake => 0.3,
            Self::Giant => 1.6,
            Self::Slime => 0.42,
            Self::Spider => 0.6,
        }
    }

    pub fn build(self, cube_mesh: MeshHandle, textures: &[TextureId]) -> ModelAsset {
        match self {
            Self::Humanoid => humanoid(textures).with_geometry(cube_mesh),
            Self::Bird => bird(textures).with_geometry(cube_mesh),
            Self::Quadruped => quadruped(textures).with_geometry(cube_mesh),
            Self::Snake => snake(textures).with_geometry(cube_mesh),
            Self::Giant => giant(textures).with_geometry(cube_mesh),
            Self::Slime => slime(textures).with_geometry(cube_mesh),
            Self::Spider => spider(textures).with_geometry(cube_mesh),
        }
    }
}

// Models

fn scaled_translated(sx: f32, sy: f32, sz: f32, tx: f32, ty: f32, tz: f32) -> [[f32; 4]; 4] {
    [
        [sx, 0.0, 0.0, 0.0],
        [0.0, sy, 0.0, 0.0],
        [0.0, 0.0, sz, 0.0],
        [tx, ty, tz, 1.0],
    ]
}

/// Textures: [head, torso, left_arm, right_arm, left_leg, right_leg]
pub fn humanoid(textures: &[TextureId]) -> ModelAsset {
    assert_eq!(textures.len(), 6, "Humanoid model requires 6 textures");

    let head = Node::identity()
        .with_name("head")
        .with_material(*textures[0])
        .with_transform(scaled_translated(0.5, 0.5, 0.5, -0.25, 1.30, -0.25));

    let torso = Node::identity()
        .with_name("torso")
        .with_material(*textures[1])
        .with_transform(scaled_translated(0.5, 0.75, 0.25, -0.25, 0.55, -0.125));

    let left_arm = Node::identity()
        .with_name("left_arm")
        .with_material(*textures[2])
        .with_transform(scaled_translated(0.25, 0.75, 0.25, -0.50, 0.55, -0.125));

    let right_arm = Node::identity()
        .with_name("right_arm")
        .with_material(*textures[3])
        .with_transform(scaled_translated(0.25, 0.75, 0.25, 0.25, 0.55, -0.125));

    let left_leg = Node::identity()
        .with_name("left_leg")
        .with_material(*textures[4])
        .with_transform(scaled_translated(0.25, 0.75, 0.25, -0.25, -0.20, -0.125));

    let right_leg = Node::identity()
        .with_name("right_leg")
        .with_material(*textures[5])
        .with_transform(scaled_translated(0.25, 0.75, 0.25, 0.00, -0.20, -0.125));

    ModelAsset::with_root(
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
pub fn quadruped(textures: &[TextureId]) -> ModelAsset {
    assert_eq!(textures.len(), 8, "Quadruped model requires 8 textures");

    let head = Node::identity()
        .with_name("head")
        .with_material(*textures[0])
        .with_transform(scaled_translated(0.4, 0.35, 0.4, 0.25, 0.75, -0.20));

    let snout = Node::identity()
        .with_name("snout")
        .with_material(*textures[1])
        .with_transform(scaled_translated(0.20, 0.15, 0.25, 0.55, 0.80, -0.125));

    let body = Node::identity()
        .with_name("body")
        .with_material(*textures[2])
        .with_transform(scaled_translated(0.75, 0.40, 0.35, -0.375, 0.40, -0.175));

    let tail = Node::identity()
        .with_name("tail")
        .with_material(*textures[3])
        .with_transform(scaled_translated(0.12, 0.35, 0.12, -0.44, 0.45, -0.06));

    let front_left_leg = Node::identity()
        .with_name("front_left_leg")
        .with_material(*textures[4])
        .with_transform(scaled_translated(0.15, 0.45, 0.15, 0.10, -0.05, -0.075));

    let front_right_leg = Node::identity()
        .with_name("front_right_leg")
        .with_material(*textures[5])
        .with_transform(scaled_translated(0.15, 0.45, 0.15, 0.28, -0.05, -0.075));

    let back_left_leg = Node::identity()
        .with_name("back_left_leg")
        .with_material(*textures[6])
        .with_transform(scaled_translated(0.15, 0.45, 0.15, -0.28, -0.05, -0.075));

    let back_right_leg = Node::identity()
        .with_name("back_right_leg")
        .with_material(*textures[7])
        .with_transform(scaled_translated(0.15, 0.45, 0.15, -0.10, -0.05, -0.075));

    ModelAsset::with_root(
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
pub fn snake(textures: &[TextureId]) -> ModelAsset {
    assert!(
        textures.len() >= 3,
        "Snake model requires at least 3 textures (head, 1+ segments, tail)"
    );

    let segment_count = textures.len() - 2;

    let head = Node::identity()
        .with_name("head")
        .with_material(*textures[0])
        .with_transform(scaled_translated(0.30, 0.25, 0.30, -0.15, 0.05, -0.15));

    let mut root = Node::identity().with_child(head);

    for i in 0..segment_count {
        let t = i as f32 / segment_count.max(1) as f32;
        let s = 0.25 - t * 0.13;
        let offset_x = (i + 1) as f32 * -(s + 0.02);
        let segment = Node::identity()
            .with_name(Box::leak(format!("segment_{i}").into_boxed_str()))
            .with_material(*textures[1 + i])
            .with_transform(scaled_translated(s, s * 0.9, s, offset_x, 0.05, -s * 0.5));
        root = root.with_child(segment);
    }

    let tail_s = 0.10;
    let tail_offset_x = -((segment_count + 1) as f32) * 0.20;
    let tail = Node::identity()
        .with_name("tail")
        .with_material(**textures.last().unwrap())
        .with_transform(scaled_translated(
            tail_s,
            tail_s * 0.8,
            tail_s,
            tail_offset_x,
            0.05,
            -tail_s * 0.5,
        ));

    ModelAsset::with_root(root.with_child(tail))
}

/// Textures: [head, beak, body, left_wing, right_wing, tail, left_leg, right_leg]
pub fn bird(textures: &[TextureId]) -> ModelAsset {
    assert_eq!(textures.len(), 8, "Bird model requires 8 textures");

    let head = Node::identity()
        .with_name("head")
        .with_material(*textures[0])
        .with_transform(scaled_translated(0.30, 0.28, 0.28, 0.10, 0.80, -0.14));

    let beak = Node::identity()
        .with_name("beak")
        .with_material(*textures[1])
        .with_transform(scaled_translated(0.18, 0.08, 0.10, 0.35, 0.86, -0.05));

    let body = Node::identity()
        .with_name("body")
        .with_material(*textures[2])
        .with_transform(scaled_translated(0.40, 0.35, 0.30, -0.20, 0.45, -0.15));

    let left_wing = Node::identity()
        .with_name("left_wing")
        .with_material(*textures[3])
        .with_transform(scaled_translated(0.45, 0.10, 0.50, -0.225, 0.60, -0.65));

    let right_wing = Node::identity()
        .with_name("right_wing")
        .with_material(*textures[4])
        .with_transform(scaled_translated(0.45, 0.10, 0.50, -0.225, 0.60, 0.15));

    let tail = Node::identity()
        .with_name("tail")
        .with_material(*textures[5])
        .with_transform(scaled_translated(0.20, 0.22, 0.10, -0.35, 0.50, -0.05));

    let left_leg = Node::identity()
        .with_name("left_leg")
        .with_material(*textures[6])
        .with_transform(scaled_translated(0.08, 0.28, 0.08, -0.04, -0.02, -0.18));

    let right_leg = Node::identity()
        .with_name("right_leg")
        .with_material(*textures[7])
        .with_transform(scaled_translated(0.08, 0.28, 0.08, 0.06, -0.02, -0.08));

    ModelAsset::with_root(
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
pub fn spider(textures: &[TextureId]) -> ModelAsset {
    assert_eq!(textures.len(), 9, "Spider model requires 9 textures");

    let head = Node::identity()
        .with_name("head")
        .with_material(*textures[0])
        .with_transform(scaled_translated(0.28, 0.22, 0.28, 0.22, 0.28, -0.14));

    let fangs = Node::identity()
        .with_name("fangs")
        .with_material(*textures[1])
        .with_transform(scaled_translated(0.10, 0.16, 0.12, 0.45, 0.20, -0.08));

    let thorax = Node::identity()
        .with_name("thorax")
        .with_material(*textures[2])
        .with_transform(scaled_translated(0.35, 0.28, 0.35, -0.175, 0.22, -0.175));

    let abdomen = Node::identity()
        .with_name("abdomen")
        .with_material(*textures[3])
        .with_transform(scaled_translated(0.40, 0.35, 0.40, -0.52, 0.18, -0.20));

    let spinnerets = Node::identity()
        .with_name("spinnerets")
        .with_material(*textures[8])
        .with_transform(scaled_translated(0.10, 0.10, 0.10, -0.68, 0.26, -0.05));

    let leg_offsets: [(f32, f32); 4] = [(0.06, 0.40), (-0.04, 0.28), (-0.14, 0.18), (-0.24, 0.12)];

    let mut root = Node::identity()
        .with_child(head)
        .with_child(fangs)
        .with_child(thorax)
        .with_child(abdomen)
        .with_child(spinnerets);

    for (i, (ox, oy)) in leg_offsets.iter().enumerate() {
        let left = Node::identity()
            .with_name(Box::leak(format!("leg_left_{i}").into_boxed_str()))
            .with_material(*textures[4 + i])
            .with_transform(scaled_translated(0.50, 0.06, 0.06, ox - 0.03, *oy, -0.50));

        let right = Node::identity()
            .with_name(Box::leak(format!("leg_right_{i}").into_boxed_str()))
            .with_material(*textures[4 + i])
            .with_transform(scaled_translated(0.50, 0.06, 0.06, ox - 0.03, *oy, 0.14));

        root = root.with_child(left).with_child(right);
    }

    ModelAsset::with_root(root)
}

/// Textures: [body_top, body_mid, body_bot, eye_left, eye_right, drip_left, drip_right]
pub fn slime(textures: &[TextureId]) -> ModelAsset {
    assert_eq!(textures.len(), 7, "Slime model requires 7 textures");

    // Three stacked tiers, widening toward the base
    let body_top = Node::identity()
        .with_name("body_top")
        .with_material(*textures[0])
        .with_transform(scaled_translated(0.55, 0.28, 0.55, -0.275, 0.70, -0.275));

    let body_mid = Node::identity()
        .with_name("body_mid")
        .with_material(*textures[1])
        .with_transform(scaled_translated(0.70, 0.32, 0.70, -0.35, 0.38, -0.35));

    let body_bot = Node::identity()
        .with_name("body_bot")
        .with_material(*textures[2])
        .with_transform(scaled_translated(0.85, 0.42, 0.85, -0.425, 0.0, -0.425));

    let eye_left = Node::identity()
        .with_name("eye_left")
        .with_material(*textures[3])
        .with_transform(scaled_translated(0.12, 0.14, 0.10, -0.10, 0.84, -0.30));

    let eye_right = Node::identity()
        .with_name("eye_right")
        .with_material(*textures[4])
        .with_transform(scaled_translated(0.12, 0.14, 0.10, 0.08, 0.84, -0.30));

    let drip_left = Node::identity()
        .with_name("drip_left")
        .with_material(*textures[5])
        .with_transform(scaled_translated(0.12, 0.10, 0.12, -0.22, -0.02, -0.28));

    let drip_right = Node::identity()
        .with_name("drip_right")
        .with_material(*textures[6])
        .with_transform(scaled_translated(0.12, 0.10, 0.12, 0.04, -0.02, -0.28));

    ModelAsset::with_root(
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
pub fn giant(textures: &[TextureId]) -> ModelAsset {
    assert_eq!(textures.len(), 10, "Giant model requires 10 textures");

    let head = Node::identity()
        .with_name("head")
        .with_material(*textures[0])
        .with_transform(scaled_translated(1.40, 1.30, 1.40, -0.70, 3.90, -0.70));

    let horn_left = Node::identity()
        .with_name("horn_left")
        .with_material(*textures[1])
        .with_transform(scaled_translated(0.30, 0.80, 0.30, -0.70, 5.00, -0.15));

    let horn_right = Node::identity()
        .with_name("horn_right")
        .with_material(*textures[2])
        .with_transform(scaled_translated(0.30, 0.80, 0.30, 0.30, 5.00, -0.15));

    let torso = Node::identity()
        .with_name("torso")
        .with_material(*textures[3])
        .with_transform(scaled_translated(1.60, 2.00, 0.80, -0.80, 1.70, -0.40));

    let shoulder_left = Node::identity()
        .with_name("shoulder_left")
        .with_material(*textures[4])
        .with_transform(scaled_translated(0.60, 0.55, 0.60, -1.55, 3.30, -0.30));

    let shoulder_right = Node::identity()
        .with_name("shoulder_right")
        .with_material(*textures[5])
        .with_transform(scaled_translated(0.60, 0.55, 0.60, 0.95, 3.30, -0.30));

    let left_arm = Node::identity()
        .with_name("left_arm")
        .with_material(*textures[6])
        .with_transform(scaled_translated(0.70, 2.00, 0.70, -1.55, 1.60, -0.35));

    let right_arm = Node::identity()
        .with_name("right_arm")
        .with_material(*textures[7])
        .with_transform(scaled_translated(0.70, 2.00, 0.70, 0.85, 1.60, -0.35));

    let left_leg = Node::identity()
        .with_name("left_leg")
        .with_material(*textures[8])
        .with_transform(scaled_translated(0.70, 1.80, 0.70, -0.75, -0.10, -0.35));

    let right_leg = Node::identity()
        .with_name("right_leg")
        .with_material(*textures[9])
        .with_transform(scaled_translated(0.70, 1.80, 0.70, 0.05, -0.10, -0.35));

    ModelAsset::with_root(
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
