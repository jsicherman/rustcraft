use egui::{Color32, Context, Id, LayerId, Order, Painter, Pos2, Rect, Stroke, ViewportId};
use egui_wgpu::Renderer;
use egui_winit::State;
use wgpu::{Device, TextureFormat};
use winit::window::Window;

use crate::{
    OverlayParticle,
    math::{face_corners_from_normal, world_to_screen},
};

pub struct DebugOverlayData {
    pub player_pos: [f32; 3],
    pub yaw_radians: f32,
    pub pitch_radians: f32,
    pub vertex_count: u32,
    pub chunk_count: u32,
    pub mesh_count: u32,
    pub model_count: u32,
    pub entity_count: u32,
    pub frames_per_second: u32,
    pub average_frame_time_ms: u128,
    pub time_of_day: f32,
}

impl DebugOverlayData {
    pub fn draw(&self, ctx: &Context) {
        egui::Window::new("Debug")
            .default_pos([10.0, 10.0])
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Pos ({:.2}, {:.2}, {:.2})",
                    self.player_pos[0], self.player_pos[1], self.player_pos[2]
                ));
                ui.label(format!(
                    "Yaw {:.1} deg   Pitch {:.1} deg",
                    self.yaw_radians.to_degrees(),
                    self.pitch_radians.to_degrees(),
                ));
                ui.label(format!("Meshes {}", self.mesh_count));
                ui.label(format!("Models {}", self.model_count));
                ui.label(format!("Entities {}", self.entity_count));
                ui.label(format!("Chunks {}", self.chunk_count));
                ui.label(format!("Vertices {}", self.vertex_count));
                ui.label(format!("FPS {}", self.frames_per_second));
                ui.label(format!("Average {} ms", self.average_frame_time_ms));
                ui.label(format!("Time of Day {:.2} hours", self.time_of_day));
            });
    }
}

pub struct Gui {
    pub context: Context,
    pub renderer: Renderer,
    pub state: State,
}

impl Gui {
    pub fn render_overlay(
        ctx: &Context,
        target: Option<[[f32; 3]; 3]>,
        view_proj: [[f32; 4]; 4],
        particles: &[OverlayParticle],
    ) {
        let screen = ctx.content_rect();
        let center = screen.center();

        let painter = ctx.layer_painter(LayerId::new(
            Order::Foreground,
            Id::new("ingame_foreground_overlay"),
        ));

        render_particles(&painter, particles, view_proj, screen);
        render_crosshairs(&painter, center);
        if let Some(target) = target {
            render_target_outline(&painter, target[0], target[1], target[2], view_proj, screen);
        }
    }
}

pub fn configure_gui(device: &Device, window: &Window, surface_format: TextureFormat) -> Gui {
    let context = Context::default();
    let state = State::new(context.clone(), ViewportId::ROOT, window, None, None, None);

    let renderer = Renderer::new(device, surface_format, Default::default());

    Gui {
        context,
        renderer,
        state,
    }
}

fn render_target_outline(
    painter: &Painter,
    min: [f32; 3],
    normal: [f32; 3],
    scale: [f32; 3],
    view_proj: [[f32; 4]; 4],
    screen: Rect,
) {
    const BLOCK_OUTLINE_COLOR: Color32 = Color32::from_rgb(5, 5, 5);
    const BLOCK_STROKE: Stroke = Stroke {
        width: 1.2,
        color: BLOCK_OUTLINE_COLOR,
    };

    let max = [min[0] + scale[0], min[1] + scale[1], min[2] + scale[2]];
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

fn render_particles(
    painter: &Painter,
    particles: &[OverlayParticle],
    view_proj: [[f32; 4]; 4],
    screen: Rect,
) {
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
}

fn render_crosshairs(painter: &Painter, center: Pos2) {
    const CROSSHAIR_SIZE: f32 = 6.0;
    const CROSSHAIR_GAP: f32 = 2.0;
    const CROSSHAIR_STROKE: Stroke = Stroke {
        width: 1.8,
        color: Color32::from_rgb(5, 5, 5),
    };

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
}
