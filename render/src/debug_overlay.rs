use egui::{Context, Window};

pub struct DebugOverlayData {
    pub player_pos: [f32; 3],
    pub yaw_radians: f32,
    pub pitch_radians: f32,
    pub index_count: u32,
    pub entity_index_count: u32,
    pub entity_count: u32,
    pub frame_time_ms: u128,
}

pub fn draw(ctx: &Context, data: &DebugOverlayData) {
    Window::new("Debug")
        .default_pos([10.0, 10.0])
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!(
                "Pos ({:.2}, {:.2}, {:.2})",
                data.player_pos[0], data.player_pos[1], data.player_pos[2]
            ));
            ui.label(format!(
                "Yaw {:.1} deg   Pitch {:.1} deg",
                data.yaw_radians.to_degrees(),
                data.pitch_radians.to_degrees(),
            ));
            ui.label(format!("Indices {}", data.index_count));
            ui.label(format!("Entity Indices {}", data.entity_index_count));
            ui.label(format!("Entity Count {}", data.entity_count));
            ui.label(format!("Frame {} ms", data.frame_time_ms));
        });
}
