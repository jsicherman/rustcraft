pub mod entity;
pub mod particles;
pub mod world;

use std::ops::Deref;

use anyhow::Error;
use chunk::WireChunk;
use renet::{Bytes, RenetClient, RenetServer};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use spatial::vectors::Vec2iChunk;

use crate::{
    entity::{ClientMessage, EntityMessage},
    world::WorldMessage,
};

pub const PROTOCOL_ID: u64 = 0xABCDEF;

pub const RENDER_DISTANCE: i32 = 8;
pub const MAX_TRANSMISSION_DISTANCE_SQ: i32 = 10;
pub const RENDER_DISTANCE_SQ: i32 = RENDER_DISTANCE * RENDER_DISTANCE;

pub const CHANNEL_WORLD: u8 = 0;
pub const CHANNEL_GAMEPLAY_RELIABLE: u8 = 1;
pub const CHANNEL_MOVEMENT_STREAM: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkId(pub u64);

impl Deref for NetworkId {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub trait ClientBound: Packet {
    fn transmit_callback(
        self,
        server: &mut RenetServer,
        to_ids: impl IntoIterator<Item = NetworkId>,
        to_positions: impl IntoIterator<Item = Vec2iChunk>,
        from_position: Option<Vec2iChunk>,
        mut callback: impl FnMut(NetworkId),
    ) {
        let channel = self.send_channel();
        let spatial = self.is_spatial();
        let descriptor = self.descriptor();
        let msg = Bytes::from(self.encode().unwrap());

        for to_id in filtered_recipients(to_ids, to_positions, from_position, spatial) {
            if server.can_send_message(*to_id, channel, msg.len()) {
                server.send_message(*to_id, channel, msg.clone());
                callback(to_id);
            } else {
                tracing::warn!("Backpressure sending {descriptor} to {to_id:?}");
            }
        }
    }

    fn transmit_multiple<O: Packet>(
        self,
        others: impl IntoIterator<Item = O>,
        server: &mut RenetServer,
        to_ids: impl IntoIterator<Item = NetworkId>,
        to_positions: impl IntoIterator<Item = Vec2iChunk>,
        from_position: Option<Vec2iChunk>,
    ) {
        let spatial = self.is_spatial();
        let msgs: Vec<_> = std::iter::once(self)
            .map(|msg| {
                (
                    msg.send_channel(),
                    msg.descriptor(),
                    Bytes::from(msg.encode().unwrap()),
                )
            })
            .chain(others.into_iter().map(|msg| {
                (
                    msg.send_channel(),
                    msg.descriptor(),
                    Bytes::from(msg.encode().unwrap()),
                )
            }))
            .collect();

        let recipients: Vec<_> =
            filtered_recipients(to_ids, to_positions, from_position, spatial).collect();

        for to_id in &recipients {
            for (channel, descriptor, msg) in &msgs {
                if server.can_send_message(**to_id, *channel, msg.len()) {
                    server.send_message(**to_id, *channel, msg.clone());
                } else {
                    tracing::warn!("Backpressure sending {descriptor} to {to_id:?}");
                }
            }
        }
    }

    /// Transmit the packet to `to_ids`. If this is a spatially-oriented packet and locations
    /// are provided, packets will not be sent to observers outside the transmission distance
    fn transmit(
        self,
        server: &mut RenetServer,
        to_ids: impl IntoIterator<Item = NetworkId>,
        to_positions: impl IntoIterator<Item = Vec2iChunk>,
        from_position: Option<Vec2iChunk>,
    ) {
        self.transmit_callback(server, to_ids, to_positions, from_position, |_| {})
    }

    fn transmit_except(
        self,
        server: &mut RenetServer,
        except: NetworkId,
        to_ids: impl IntoIterator<Item = NetworkId>,
        to_positions: impl IntoIterator<Item = Vec2iChunk>,
        from_position: Option<Vec2iChunk>,
    ) {
        self.transmit(
            server,
            to_ids.into_iter().filter(|id| id != &except),
            to_positions,
            from_position,
        );
    }
}

pub trait ServerBound: Packet {
    fn transmit(self, client: &mut RenetClient) {
        let channel = self.send_channel();
        let msg = Bytes::from(self.encode().unwrap());

        if client.can_send_message(channel, msg.len()) {
            client.send_message(channel, msg);
        } else {
            tracing::warn!("Backpressure sending message from client");
        }
    }
}

pub trait Packet: Serialize + DeserializeOwned {
    type Output: DeserializeOwned;

    fn descriptor(&self) -> &'static str {
        std::any::type_name_of_val(self)
    }

    fn encode(self) -> Result<Vec<u8>, Error> {
        let serialized = bincode::serde::encode_to_vec(self, bincode::config::standard())?;
        let compressed = zstd::encode_all(serialized.as_slice(), 0)?;

        Ok(compressed)
    }

    fn decode(bytes: &[u8]) -> Result<Self::Output, Error> {
        let decompressed = zstd::decode_all(bytes)?;

        let (value, _) =
            bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())?;

        Ok(value)
    }

    fn send_channel(&self) -> u8 {
        Self::channel()
    }

    fn receive_channels() -> Vec<u8> {
        vec![Self::channel()]
    }

    fn is_spatial(&self) -> bool {
        false
    }

    fn channel() -> u8;
}

fn filtered_recipients<'a>(
    to_ids: impl IntoIterator<Item = NetworkId> + 'a,
    to_positions: impl IntoIterator<Item = Vec2iChunk> + 'a,
    from_position: Option<Vec2iChunk>,
    spatial: bool,
) -> impl Iterator<Item = NetworkId> + 'a {
    let mut to_positions = to_positions.into_iter();

    to_ids.into_iter().filter(move |_to_id| {
        if spatial && let Some(from_pos) = from_position {
            to_positions.next().is_none_or(|to_pos| {
                (from_pos - to_pos).length_sq() <= MAX_TRANSMISSION_DISTANCE_SQ
            })
        } else {
            true
        }
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    World(WorldMessage),
    Entity(EntityMessage),
}

impl ClientBound for WorldMessage {}
impl ClientBound for EntityMessage {}
impl ServerBound for ClientMessage {}

impl Packet for WorldMessage {
    type Output = ServerMessage;

    fn channel() -> u8 {
        CHANNEL_WORLD
    }

    fn is_spatial(&self) -> bool {
        match self {
            Self::ServerTime(_) => false,
            Self::ChunkData(_) | Self::BlockModification { .. } | Self::ParticleSpawn { .. } => {
                true
            }
        }
    }

    fn encode(self) -> Result<Vec<u8>, Error> {
        let msg = ServerMessage::World(self);

        let serialized = match msg {
            ServerMessage::World(WorldMessage::ChunkData(wire)) => wire.into_bytes(),
            other => bincode::serde::encode_to_vec(other, bincode::config::standard())?,
        };

        let compressed = zstd::encode_all(serialized.as_slice(), 0)?;

        Ok(compressed)
    }

    fn decode(bytes: &[u8]) -> Result<Self::Output, Error> {
        let decompressed = zstd::decode_all(bytes)?;

        // FIXME: wasteful
        if let Ok(wire_chunk) = WireChunk::from_bytes(&decompressed) {
            Ok(ServerMessage::World(WorldMessage::ChunkData(wire_chunk)))
        } else {
            let (value, _) =
                bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())?;

            Ok(value)
        }
    }
}
impl Packet for ClientMessage {
    type Output = Self;

    fn channel() -> u8 {
        CHANNEL_GAMEPLAY_RELIABLE
    }

    fn send_channel(&self) -> u8 {
        match self {
            Self::Move(_) => CHANNEL_MOVEMENT_STREAM,
            Self::Look(_) | Self::InteractBlock { .. } | Self::RemodelEntity { .. } => {
                CHANNEL_GAMEPLAY_RELIABLE
            }
        }
    }

    fn receive_channels() -> Vec<u8> {
        vec![CHANNEL_GAMEPLAY_RELIABLE, CHANNEL_MOVEMENT_STREAM]
    }
}
impl Packet for EntityMessage {
    type Output = ServerMessage;

    fn channel() -> u8 {
        CHANNEL_GAMEPLAY_RELIABLE
    }

    fn send_channel(&self) -> u8 {
        match self {
            Self::Move { .. } | Self::GuidedMove { .. } => CHANNEL_MOVEMENT_STREAM,
            Self::ClientConnect { .. }
            | Self::Spawn { .. }
            | Self::Despawn(_)
            | Self::Look { .. }
            | Self::Remodel { .. }
            | Self::BlockEntityUpdate { .. }
            | Self::GuidedLook { .. } => CHANNEL_GAMEPLAY_RELIABLE,
        }
    }

    fn receive_channels() -> Vec<u8> {
        vec![CHANNEL_GAMEPLAY_RELIABLE, CHANNEL_MOVEMENT_STREAM]
    }

    fn encode(self) -> Result<Vec<u8>, Error> {
        let msg = ServerMessage::Entity(self);

        let serialized = bincode::serde::encode_to_vec(msg, bincode::config::standard())?;
        let compressed = zstd::encode_all(serialized.as_slice(), 0)?;

        Ok(compressed)
    }

    fn decode(bytes: &[u8]) -> Result<Self::Output, Error> {
        let decompressed = zstd::decode_all(bytes)?;

        let (value, _) =
            bincode::serde::decode_from_slice(&decompressed, bincode::config::standard())?;

        Ok(value)
    }
}
