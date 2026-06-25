use std::array::IntoIter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    Human,
    Horse,
    Snake,
    Bird,
    Giant,
    Slime,
    Spider,
}

impl EntityType {
    pub fn iter() -> IntoIter<Self, 7> {
        [
            Self::Human,
            Self::Snake,
            Self::Bird,
            Self::Horse,
            Self::Giant,
            Self::Slime,
            Self::Spider,
        ]
        .into_iter()
    }
}
