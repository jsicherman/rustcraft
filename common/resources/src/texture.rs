use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::block::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextureId(pub u8);

impl Deref for TextureId {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TextureId {
    pub const MAX: u8 = 3;
    pub const OFFSET: u8 = BlockId::MAX;

    pub fn iter() -> impl Iterator<Item = TextureId> {
        (0..Self::MAX).map(|id| TextureId(id + Self::OFFSET))
    }
}
