use serde::{Deserialize, Serialize};

pub(crate) const NO_ENTITY: u16 = u16::MAX;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlockEntityData {
    Sign { lines: [String; 4] },
}

impl BlockEntityData {
    pub(crate) fn tick(&mut self) {
        match self {
            BlockEntityData::Sign { .. } => {
                // Signs don't tick
            }
        }
    }
}
