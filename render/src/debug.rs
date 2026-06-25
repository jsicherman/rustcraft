use egui::{Context, Window};

pub struct DebugOverlayData {
    pub player_pos: [f32; 3],
    pub yaw_radians: f32,
    pub pitch_radians: f32,
    pub vertex_count: u32,
    pub chunk_count: u32,
    pub mesh_count: u32,
    pub model_count: u32,
    pub entity_count: u32,
    pub frame_time_ms: u128,
    pub average_frame_time_ms: u128,
    pub time_of_day: f32,
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
            ui.label(format!("Meshes {}", data.mesh_count));
            ui.label(format!("Models {}", data.model_count));
            ui.label(format!("Entities {}", data.entity_count));
            ui.label(format!("Chunks {}", data.chunk_count));
            ui.label(format!("Vertices {}", data.vertex_count));
            ui.label(format!("Last {} ms", data.frame_time_ms));
            ui.label(format!("Average {} ms", data.average_frame_time_ms));
            ui.label(format!("Time of Day {:.2} hours", data.time_of_day));
        });
}
