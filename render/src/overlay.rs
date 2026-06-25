use egui::{Color32, Context, Id, LayerId, Order, Pos2, Rect, Stroke};

use crate::OverlayParticle;

pub fn foreground_overlays(
    ctx: &Context,
    target_position_normal: Option<([f32; 3], [f32; 3])>,
    view_proj: [[f32; 4]; 4],
    particles: &[OverlayParticle],
) {
    const CROSSHAIR_SIZE: f32 = 6.0;
    const CROSSHAIR_GAP: f32 = 2.0;
    const BLOCK_OUTLINE_COLOR: Color32 = Color32::from_rgb(5, 5, 5);
    const BLOCK_STROKE: Stroke = Stroke {
        width: 1.2,
        color: BLOCK_OUTLINE_COLOR,
    };
    const CROSSHAIR_STROKE: Stroke = Stroke {
        width: 1.8,
        color: Color32::from_rgb(5, 5, 5),
    };

    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(LayerId::new(
        Order::Foreground,
        Id::new("ingame_foreground_overlay"),
    ));

    let center = screen.center();

    for particle in particles {
        let Some(screen_pos) = world_to_screen(particle.position, view_proj, screen) else {
            continue;
        };

        let color = Color32::from_rgba_premultiplied(
            particle.color[0],
            particle.color[1],
            particle.color[2],
            particle.color[3],
        );

        painter.circle_filled(screen_pos, particle.radius.max(0.4), color);
    }

    painter.line_segment(
        [
            Pos2::new(center.x - CROSSHAIR_SIZE, center.y),
            Pos2::new(center.x - CROSSHAIR_GAP, center.y),
        ],
        CROSSHAIR_STROKE,
    );
    painter.line_segment(
        [
            Pos2::new(center.x + CROSSHAIR_GAP, center.y),
            Pos2::new(center.x + CROSSHAIR_SIZE, center.y),
        ],
        CROSSHAIR_STROKE,
    );
    painter.line_segment(
        [
            Pos2::new(center.x, center.y - CROSSHAIR_SIZE),
            Pos2::new(center.x, center.y - CROSSHAIR_GAP),
        ],
        CROSSHAIR_STROKE,
    );
    painter.line_segment(
        [
            Pos2::new(center.x, center.y + CROSSHAIR_GAP),
            Pos2::new(center.x, center.y + CROSSHAIR_SIZE),
        ],
        CROSSHAIR_STROKE,
    );

    let Some((min, normal)) = target_position_normal else {
        return;
    };

    let max = [min[0] + 1.0, min[1] + 1.0, min[2] + 1.0];
    let corners: [[f32; 3]; 8] = [
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], max[1], min[2]],
        [min[0], max[1], min[2]],
        [min[0], min[1], max[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ];

    let projected = corners.map(|corner| world_to_screen(corner, view_proj, screen));

    let Some(face) = face_corners_from_normal(normal) else {
        return;
    };

    for i in 0..4 {
        let a = face[i];
        let b = face[(i + 1) % 4];

        let (Some(pa), Some(pb)) = (projected[a], projected[b]) else {
            continue;
        };
        painter.line_segment([pa, pb], BLOCK_STROKE);
    }
}

fn face_corners_from_normal(normal: [f32; 3]) -> Option<[usize; 4]> {
    let [nx, ny, nz] = normal;
    let ax = nx.abs();
    let ay = ny.abs();
    let az = nz.abs();

    if ax < 0.5 && ay < 0.5 && az < 0.5 {
        return None;
    }

    if ax >= ay && ax >= az {
        if nx > 0.0 {
            Some([1, 2, 6, 5])
        } else {
            Some([0, 3, 7, 4])
        }
    } else if ay >= az {
        if ny > 0.0 {
            Some([3, 2, 6, 7])
        } else {
            Some([0, 1, 5, 4])
        }
    } else if nz > 0.0 {
        Some([4, 5, 6, 7])
    } else {
        Some([0, 1, 2, 3])
    }
}

fn mul_mat4_vec4(m: [[f32; 4]; 4], v: [f32; 4]) -> [f32; 4] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2] + m[3][0] * v[3],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2] + m[3][1] * v[3],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2] + m[3][2] * v[3],
        m[0][3] * v[0] + m[1][3] * v[1] + m[2][3] * v[2] + m[3][3] * v[3],
    ]
}

fn world_to_screen(world: [f32; 3], view_proj: [[f32; 4]; 4], screen: Rect) -> Option<Pos2> {
    let clip = mul_mat4_vec4(view_proj, [world[0], world[1], world[2], 1.0]);
    if clip[3] <= 0.0001 {
        return None;
    }

    let ndc = [clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]];
    if ndc[0] < -1.2
        || ndc[0] > 1.2
        || ndc[1] < -1.2
        || ndc[1] > 1.2
        || ndc[2] < -1.0
        || ndc[2] > 1.0
    {
        return None;
    }

    let x = screen.left() + (ndc[0] * 0.5 + 0.5) * screen.width();
    let y = screen.top() + (0.5 - ndc[1] * 0.5) * screen.height();
    Some(Pos2::new(x, y))
}
