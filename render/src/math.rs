use egui::{Pos2, Rect};

pub fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if len_sq <= f32::EPSILON {
        [0.0, 1.0, 0.0]
    } else {
        let inv_len = 1.0 / len_sq.sqrt();
        [v[0] * inv_len, v[1] * inv_len, v[2] * inv_len]
    }
}

pub fn face_corners_from_normal(normal: [f32; 3]) -> Option<[usize; 4]> {
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

pub fn mul_mat4_vec4(m: [[f32; 4]; 4], v: [f32; 4]) -> [f32; 4] {
    [
        m[0][0] * v[0] + m[1][0] * v[1] + m[2][0] * v[2] + m[3][0] * v[3],
        m[0][1] * v[0] + m[1][1] * v[1] + m[2][1] * v[2] + m[3][1] * v[3],
        m[0][2] * v[0] + m[1][2] * v[1] + m[2][2] * v[2] + m[3][2] * v[3],
        m[0][3] * v[0] + m[1][3] * v[1] + m[2][3] * v[2] + m[3][3] * v[3],
    ]
}

pub fn world_to_screen(world: [f32; 3], view_proj: [[f32; 4]; 4], screen: Rect) -> Option<Pos2> {
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

pub fn translate(x: f32, y: f32, z: f32) -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [x, y, z, 1.0],
    ]
}

pub fn transform_with_pivot(local_transform: [[f32; 4]; 4], pivot: [f32; 3]) -> [[f32; 4]; 4] {
    // To rotate around a pivot point: translate(pivot) * transform * translate(-pivot)
    let translate_to_pivot = translate(pivot[0], pivot[1], pivot[2]);
    let translate_from_pivot = translate(-pivot[0], -pivot[1], -pivot[2]);
    mat4_mul(
        translate_to_pivot,
        mat4_mul(local_transform, translate_from_pivot),
    )
}

pub fn mat4_mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
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
