pub mod camera;
mod chunk;
mod event_handler;
mod frame_handler;
mod key_handler;
mod particles;
mod renderer;
pub mod settings;
pub mod world;

use crate::{
    camera::Camera, particles::ParticleSystem, renderer::RenderState, settings::AppConfig,
    world::ChunkCache,
};
use ::world::TimeOfDay;
use ecs::{Entity, EntityOrientation, MovementIntent, World};
use protocol::{NetworkId, PROTOCOL_ID};
use render::{
    Renderer, cube,
    model::{ModelHandle, RenderCommandGpu},
};
use renet::RenetClient;
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};
use resources::{
    ResourcePack,
    entity::{EntityType, ModelDefinition},
};
use spatial::vectors::Vec2iChunk;
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Instant, SystemTime},
};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

#[derive(Default)]
pub struct App {
    state: Option<AppState>,
    config: AppConfig,
}

impl App {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }
}

pub struct AppState {
    window: Arc<Window>,
    renderer: Renderer,
    render_queue: Vec<RenderCommandGpu>,

    client: RenetClient,
    transport: NetcodeClientTransport,

    last_update: Instant,
    frame_timer: FrameTimer,

    resource_pack: ResourcePack,

    chunk_state: ChunkCache,
    entity_state: RenderState,

    camera: Camera,
    world: World,

    local_player: Option<(NetworkId, Option<Entity>)>,
    network_to_local: HashMap<NetworkId, Entity>,

    pressed_keys: HashSet<KeyCode>,
    pressed_mouse_buttons: HashSet<MouseButton>,

    previous_state: PreviousState,

    time_of_day: TimeOfDay,
    particles: ParticleSystem,
}

struct FrameTimer {
    samples: [u128; 60],
    index: usize,
    count: usize,
}

impl Default for FrameTimer {
    fn default() -> Self {
        Self {
            samples: [0; 60],
            index: 0,
            count: 0,
        }
    }
}

impl FrameTimer {
    fn push(&mut self, dt: u128) {
        self.samples[self.index] = dt;
        self.index = (self.index + 1) % 60;
        self.count = (self.count + 1).min(60);
    }

    fn avg(&self) -> u128 {
        (self.samples[..self.count].iter().sum::<u128>() as f64 / self.count as f64) as u128
    }
}

#[derive(Default)]
pub struct PreviousState {
    pub intent: Option<MovementIntent>,
    pub orientation: Option<EntityOrientation>,
    pub down: HashMap<MouseButton, Instant>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_inner_size(PhysicalSize::new(1280u32, 720u32)),
                )
                .unwrap(),
        );
        window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
            .unwrap();
        window.set_cursor_visible(false);

        let resource_pack = ResourcePack::load();

        let mut renderer = pollster::block_on(render::init(
            Arc::clone(&window),
            resource_pack.block_resources(),
            resource_pack.texture_resources(),
        ));

        let cube_mesh = cube(&mut renderer);
        // FIXME: override texture instead of new model?
        for entity in EntityType::ALL {
            let model: ModelDefinition = entity.into();
            renderer.insert_model(
                ModelHandle::from(entity.handle()),
                model.build(cube_mesh, entity.textures()),
            );
        }

        let client = RenetClient::new(Default::default());
        let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        let transport = NetcodeClientTransport::new(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap(),
            ClientAuthentication::Unsecure {
                protocol_id: PROTOCOL_ID,
                client_id: rand::random(),
                server_addr: format!("{}:{}", self.config.server.address, self.config.server.port)
                    .parse::<SocketAddr>()
                    .unwrap(),
                user_data: None,
            },
            socket,
        )
        .unwrap();

        let camera = Camera::new();
        let world = World::new();

        self.state = Some(AppState {
            window,
            renderer,
            camera,
            client,
            transport,
            local_player: None,
            world,
            resource_pack,
            chunk_state: Default::default(),
            entity_state: Default::default(),
            network_to_local: Default::default(),
            pressed_keys: Default::default(),
            pressed_mouse_buttons: Default::default(),
            previous_state: Default::default(),
            render_queue: Default::default(),
            frame_timer: Default::default(),
            last_update: Instant::now(),
            particles: Default::default(),
            time_of_day: TimeOfDay::default(),
        });
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let Some(state) = &mut self.state else { return };

        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            state.camera.handle_cursor_moved(dx, dy);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };

        let event_consumed = state.renderer.handle_window_event(&state.window, &event);

        match event {
            WindowEvent::Focused(true) => {
                let size = state.window.inner_size();
                let center = PhysicalPosition::new(size.width / 2, size.height / 2);
                let _ = state.window.set_cursor_position(center);

                state
                    .window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| state.window.set_cursor_grab(CursorGrabMode::Confined))
                    .ok();
                state.window.set_cursor_visible(false);
                state.camera.reset_cursor_delta();
            }
            WindowEvent::Focused(false) => {
                state.window.set_cursor_grab(CursorGrabMode::None).ok();
                state.window.set_cursor_visible(true);
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state: key_state,
                        ..
                    },
                ..
            } if !event_consumed => match key_state {
                ElementState::Pressed => {
                    state.pressed_keys.insert(key);

                    if key == KeyCode::Escape {
                        event_loop.exit();
                    }
                }
                ElementState::Released => {
                    state.pressed_keys.remove(&key);
                }
            },
            WindowEvent::MouseInput {
                state: element_state,
                button,
                ..
            } if !event_consumed => {
                if element_state.is_pressed() {
                    state.pressed_mouse_buttons.insert(button);
                } else {
                    state.pressed_mouse_buttons.remove(&button);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now - state.last_update;
                state.frame_timer.push(dt.as_millis());
                state.last_update = now;

                state.particles.tick(dt);

                let (position, orientation, _, properties) = state.process_inputs(dt);

                state.process_gravity(dt);

                let chunk_position = Vec2iChunk::from(position);

                match state.transport.send_packets(&mut state.client) {
                    Ok(_) => {}
                    Err(err) => {
                        tracing::error!("Failed to send packets: {err}");
                    }
                }
                state.receive_chunk_messages(chunk_position);
                state.receive_entity_messages(position);

                state.request_chunk_frames(chunk_position);
                state.request_entity_frames(position);

                state.receive_chunk_frames(chunk_position);

                state.render_frame(position, &properties, orientation, dt);

                state.transport.send_packets(&mut state.client).ok();
                state.window.request_redraw();
            }
            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
            }
            _ => {}
        }
    }
}
