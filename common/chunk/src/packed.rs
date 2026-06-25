use serde::{Deserialize, Serialize, de::Error};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackedIndices {
    bits_per_index: u8,
    len: usize,
    data: Vec<u8>,
}

impl PackedIndices {
    pub fn bits_for_palette_len(palette_len: usize) -> u8 {
        debug_assert!(palette_len > 0);
        let bits = (usize::BITS - palette_len.saturating_sub(1).leading_zeros()) as u8;
        bits.max(1)
    }

    pub fn packed_len(len: usize, bits_per_index: u8) -> usize {
        (len * bits_per_index as usize).div_ceil(8)
    }

    pub fn new(len: usize, palette_len: usize) -> Self {
        let bits_per_index = Self::bits_for_palette_len(palette_len);
        Self {
            bits_per_index,
            len,
            data: vec![0; Self::packed_len(len, bits_per_index)],
        }
    }

    pub fn filled(len: usize, palette_len: usize, value: u16) -> Self {
        let mut packed = Self::new(len, palette_len);
        for index in 0..len {
            packed.set(index, value);
        }
        packed
    }

    pub fn from_indices(indices: &[u16], palette_len: usize) -> Self {
        let mut packed = Self::new(indices.len(), palette_len);
        for (index, &value) in indices.iter().enumerate() {
            packed.set(index, value);
        }
        packed
    }

    pub fn from_parts(
        bits_per_index: u8,
        len: usize,
        data: Vec<u8>,
    ) -> Result<Self, serde::de::value::Error> {
        if bits_per_index == 0 || bits_per_index > 16 {
            return Err(Error::custom("Invalid bits per index"));
        }

        let expected_len = Self::packed_len(len, bits_per_index);
        if data.len() != expected_len {
            return Err(Error::custom("Invalid packed index length"));
        }

        Ok(Self {
            bits_per_index,
            len,
            data,
        })
    }

    pub fn bits_per_index(&self) -> u8 {
        self.bits_per_index
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn repacked(&self, palette_len: usize) -> Self {
        let mut packed = Self::new(self.len, palette_len);
        for index in 0..self.len {
            packed.set(index, self.get(index));
        }
        packed
    }

    pub fn get(&self, index: usize) -> u16 {
        debug_assert!(index < self.len);

        let bits = self.bits_per_index as usize;
        let bit_offset = index * bits;
        let byte_offset = bit_offset / 8;
        let shift = bit_offset % 8;

        let mut chunk = 0u32;
        for i in 0..4 {
            if let Some(&byte) = self.data.get(byte_offset + i) {
                chunk |= (byte as u32) << (i * 8);
            }
        }

        let mask = if self.bits_per_index == 16 {
            u32::from(u16::MAX)
        } else {
            (1u32 << self.bits_per_index) - 1
        };

        ((chunk >> shift) & mask) as u16
    }

    pub fn set(&mut self, index: usize, value: u16) {
        debug_assert!(index < self.len);

        let capacity = if self.bits_per_index == 16 {
            usize::from(u16::MAX) + 1
        } else {
            1usize << self.bits_per_index
        };
        debug_assert!((value as usize) < capacity);

        let bits = self.bits_per_index as usize;
        let bit_offset = index * bits;
        let byte_offset = bit_offset / 8;
        let shift = bit_offset % 8;

        let mut chunk = 0u32;
        for i in 0..4 {
            if let Some(&byte) = self.data.get(byte_offset + i) {
                chunk |= (byte as u32) << (i * 8);
            }
        }

        let mask = if self.bits_per_index == 16 {
            u32::from(u16::MAX)
        } else {
            (1u32 << self.bits_per_index) - 1
        };

        chunk &= !(mask << shift);
        chunk |= (u32::from(value) & mask) << shift;

        for i in 0..4 {
            if let Some(byte) = self.data.get_mut(byte_offset + i) {
                *byte = ((chunk >> (i * 8)) & 0xFF) as u8;
            }
        }
    }
}
