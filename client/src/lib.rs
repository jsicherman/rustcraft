pub mod camera;
mod chunk;
mod event_handler;
mod frame_handler;
mod key_handler;
mod renderer;
pub mod settings;
pub mod world;

use crate::{camera::Camera, renderer::RenderState, settings::AppConfig, world::ChunkCache};
use block::TexturePack;
use ecs::{Entity, EntityOrientation, MovementIntent, World};
use entity::EntityType;
use model::ModelDefinition;
use protocol::{NetworkId, PROTOCOL_ID};
use render::{Renderer, model::RenderCommandGpu};
use renet::RenetClient;
use renet_netcode::{ClientAuthentication, NetcodeClientTransport};
use spatial::vectors::Vec2iChunk;
use std::{
    collections::{HashMap, HashSet},
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Instant, SystemTime},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
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

    texture_pack: TexturePack,

    chunk_state: ChunkCache,
    entity_state: RenderState,

    camera: Camera,
    world: World,

    local_player: Option<(NetworkId, Option<Entity>)>,
    network_to_local: HashMap<NetworkId, Entity>,

    pressed_keys: HashSet<KeyCode>,

    previous_state: PreviousState,
}

#[derive(Default)]
pub struct PreviousState {
    pub intent: Option<MovementIntent>,
    pub orientation: Option<EntityOrientation>,
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

        let texture_pack = TexturePack::load();

        let mut renderer = pollster::block_on(render::init(Arc::clone(&window), &texture_pack));

        let cube_mesh = renderer.cube();
        for definition in ModelDefinition::iter() {
            renderer.insert_model(
                definition.handle(),
                definition.build(cube_mesh, texture_pack.get_textures(EntityType::Human)),
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
            texture_pack,
            chunk_state: Default::default(),
            entity_state: Default::default(),
            network_to_local: Default::default(),
            pressed_keys: Default::default(),
            previous_state: Default::default(),
            render_queue: Default::default(),
            last_update: Instant::now(),
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };

        let event_consumed = state.renderer.handle_window_event(&state.window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorEntered { .. } if !event_consumed => {
                state
                    .window
                    .set_cursor_grab(CursorGrabMode::Confined)
                    .unwrap();
            }
            WindowEvent::CursorLeft { .. } if !event_consumed => {
                let _ = state.window.set_cursor_grab(CursorGrabMode::None);
            }
            WindowEvent::CursorMoved { mut position, .. } if !event_consumed => {
                let size = state.window.inner_size();

                let width = size.width as f64;

                let mut wrapped = false;
                if position.x < 1.0 {
                    wrapped = true;
                    position.x = width - 4.0;
                } else if position.x > width - 3.0 {
                    wrapped = true;
                    position.x = 2.0;
                }

                if wrapped {
                    let _ = state.window.set_cursor_position(position);
                    state.camera.reset_cursor_position(position.x, position.y);
                } else {
                    state.camera.handle_cursor_moved(position.x, position.y);
                }
            }
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
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now - state.last_update;
                state.last_update = now;

                let (position, orientation) = state.process_inputs(dt);
                let chunk_position = Vec2iChunk::from(position);

                state.transport.send_packets(&mut state.client).ok();

                state.receive_chunk_messages(chunk_position);
                state.receive_entity_messages(position);

                state.request_chunk_frames(chunk_position);
                state.request_entity_frames(position);
                state.receive_chunk_frames(chunk_position);

                state.render_frame(position, orientation, dt);

                state.window.request_redraw();
            }
            WindowEvent::Resized(size) => {
                state.renderer.resize(size.width, size.height);
            }
            _ => {}
        }
    }
}
