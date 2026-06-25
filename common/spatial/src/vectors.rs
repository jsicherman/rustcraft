use serde::{Deserialize, Serialize};
use std::ops::{Bound, RangeBounds};

use crate::{
    CHUNK_SIZE,
    aabb::{Aabb, AxisAlignedBoundingBox},
};

mod sealed {
    pub trait Sealed {}
}

pub trait CoordSpace: sealed::Sealed + Copy + Clone + std::fmt::Debug + 'static {}

/// World-space coordinates (unbounded).
#[derive(
    Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Global;

/// Chunk-local coordinates: x and z are remapped to `[0, CHUNK_SIZE)`, y is
/// unchanged (full world height).
#[derive(
    Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Local;

/// Chunk-grid coordinates: x and z are floored to `original / CHUNK_SIZE`
/// (i.e. which chunk, not position within it). y is unused / zero.
#[derive(
    Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Chunk;

impl sealed::Sealed for Global {}
impl sealed::Sealed for Local {}
impl sealed::Sealed for Chunk {}
impl CoordSpace for Global {}
impl CoordSpace for Local {}
impl CoordSpace for Chunk {}

/// Convert a vector from one coordinate space to another.
pub trait IntoSpace<Target: CoordSpace> {
    type Output;
    fn into_space(self) -> Self::Output;
}

macro_rules! __vec_index {
    ($self:expr, $i:expr; $f0:ident) => {
        match $i {
            0 => &$self.$f0,
            n => panic!("index {n} out of bounds for length 1"),
        }
    };
    ($self:expr, $i:expr; $f0:ident, $f1:ident) => {
        match $i {
            0 => &$self.$f0,
            1 => &$self.$f1,
            n => panic!("index {n} out of bounds for length 2"),
        }
    };
    ($self:expr, $i:expr; $f0:ident, $f1:ident, $f2:ident) => {
        match $i {
            0 => &$self.$f0,
            1 => &$self.$f1,
            2 => &$self.$f2,
            n => panic!("index {n} out of bounds for length 3"),
        }
    };
    ($self:expr, $i:expr; $f0:ident, $f1:ident, $f2:ident, $f3:ident) => {
        match $i {
            0 => &$self.$f0,
            1 => &$self.$f1,
            2 => &$self.$f2,
            3 => &$self.$f3,
            n => panic!("index {n} out of bounds for length 4"),
        }
    };
}

macro_rules! __vec_index_mut {
    ($self:expr, $i:expr; $f0:ident) => {
        match $i {
            0 => &mut $self.$f0,
            n => panic!("index {n} out of bounds for length 1"),
        }
    };
    ($self:expr, $i:expr; $f0:ident, $f1:ident) => {
        match $i {
            0 => &mut $self.$f0,
            1 => &mut $self.$f1,
            n => panic!("index {n} out of bounds for length 2"),
        }
    };
    ($self:expr, $i:expr; $f0:ident, $f1:ident, $f2:ident) => {
        match $i {
            0 => &mut $self.$f0,
            1 => &mut $self.$f1,
            2 => &mut $self.$f2,
            n => panic!("index {n} out of bounds for length 3"),
        }
    };
    ($self:expr, $i:expr; $f0:ident, $f1:ident, $f2:ident, $f3:ident) => {
        match $i {
            0 => &mut $self.$f0,
            1 => &mut $self.$f1,
            2 => &mut $self.$f2,
            3 => &mut $self.$f3,
            n => panic!("index {n} out of bounds for length 4"),
        }
    };
}

/// A fixed-size vector struct parameterised by a `CoordSpace`.
///
/// # Syntax
/// ```ignore
/// define_vec! {
///     #[derive(...)]
///     pub struct Name<S>: ScalarType { field0, field1, … }
///     ; up = Name { field0: val, … }   // optional
/// }
/// ```
macro_rules! define_vec {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident <S> : $scalar:ty {
            $($field:ident),+ $(,)?
        }
        $( ; up = $up_val:expr )?
    ) => {
        $(#[$meta])*
        $vis struct $name<S: CoordSpace = Global> {
            $($field: $scalar,)+
            _space: std::marker::PhantomData<S>,
        }

        impl<S: CoordSpace> std::fmt::Debug for $name<S> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name))
                    $(.field(stringify!($field), &self.$field))+
                    .finish()
            }
        }

        impl<S: CoordSpace> $name<S> {
            #[inline]
            pub const fn new($($field: $scalar),+) -> Self {
                Self { $($field,)+ _space: std::marker::PhantomData }
            }

            pub const ZERO: Self = Self {
                $($field: 0 as $scalar,)+
                _space: std::marker::PhantomData,
            };

            $(
                pub const UP: Self = $up_val;
            )?

            $(
                #[inline]
                pub fn $field(self) -> $scalar { self.$field }
            )+

            #[inline]
            pub fn reinterpret<T: CoordSpace>(self) -> $name<T> {
                $name::<T> { $($field: self.$field,)+ _space: std::marker::PhantomData }
            }

            #[inline]
            pub fn floor(self) -> Self {
                Self::new($((self.$field as f32).floor() as $scalar),+)
            }

            #[inline]
            pub fn ceil(self) -> Self {
                Self::new($((self.$field as f32).ceil() as $scalar),+)
            }
        }

        impl<S: CoordSpace> std::ops::Add for $name<S> {
            type Output = Self;
            #[inline]
            fn add(self, rhs: Self) -> Self {
                Self::new($(self.$field + rhs.$field),+)
            }
        }
        impl<S: CoordSpace> std::ops::AddAssign for $name<S> {
            #[inline]
            fn add_assign(&mut self, rhs: Self) { $(self.$field += rhs.$field;)+ }
        }

        impl<S: CoordSpace> std::ops::Sub for $name<S> {
            type Output = Self;
            #[inline]
            fn sub(self, rhs: Self) -> Self {
                Self::new($(self.$field - rhs.$field),+)
            }
        }
        impl<S: CoordSpace> std::ops::SubAssign for $name<S> {
            #[inline]
            fn sub_assign(&mut self, rhs: Self) { $(self.$field -= rhs.$field;)+ }
        }

        impl<S: CoordSpace> std::ops::Mul for $name<S> {
            type Output = Self;
            #[inline]
            fn mul(self, rhs: Self) -> Self {
                Self::new($(self.$field * rhs.$field),+)
            }
        }
        impl<S: CoordSpace> std::ops::MulAssign for $name<S> {
            #[inline]
            fn mul_assign(&mut self, rhs: Self) { $(self.$field *= rhs.$field;)+ }
        }

        impl<S: CoordSpace> std::ops::Div for $name<S> {
            type Output = Self;
            #[inline]
            fn div(self, rhs: Self) -> Self {
                Self::new($(self.$field / rhs.$field),+)
            }
        }
        impl<S: CoordSpace> std::ops::DivAssign for $name<S> {
            #[inline]
            fn div_assign(&mut self, rhs: Self) { $(self.$field /= rhs.$field;)+ }
        }

        impl<S: CoordSpace> std::ops::Neg for $name<S> {
            type Output = Self;
            #[inline]
            fn neg(self) -> Self { Self::new($(-self.$field),+) }
        }

        impl<S: CoordSpace> std::ops::Mul<$scalar> for $name<S> {
            type Output = Self;
            #[inline]
            fn mul(self, s: $scalar) -> Self { Self::new($(self.$field * s),+) }
        }
        impl<S: CoordSpace> std::ops::MulAssign<$scalar> for $name<S> {
            #[inline]
            fn mul_assign(&mut self, s: $scalar) { $(self.$field *= s;)+ }
        }

        impl<S: CoordSpace> std::ops::Div<$scalar> for $name<S> {
            type Output = Self;
            #[inline]
            fn div(self, s: $scalar) -> Self { Self::new($(self.$field / s),+) }
        }
        impl<S: CoordSpace> std::ops::DivAssign<$scalar> for $name<S> {
            #[inline]
            fn div_assign(&mut self, s: $scalar) { $(self.$field /= s;)+ }
        }

        impl<S: CoordSpace> std::ops::Index<usize> for $name<S> {
            type Output = $scalar;
            #[inline]
            fn index(&self, idx: usize) -> &$scalar {
                __vec_index!(self, idx; $($field),+)
            }
        }
        impl<S: CoordSpace> std::ops::IndexMut<usize> for $name<S> {
            #[inline]
            fn index_mut(&mut self, idx: usize) -> &mut $scalar {
                __vec_index_mut!(self, idx; $($field),+)
            }
        }

        impl<S: CoordSpace> $name<S> {
            /// Build from a slice. Panics if `s.len()` < number of fields.
            #[inline]
            #[allow(unused_assignments)]
            pub fn from_slice(s: &[$scalar]) -> Self {
                let mut i = 0usize;
                Self::new($({ let v = s[i]; i += 1; let _ = stringify!($field); v }),+)
            }

            /// Collect fields into a `Vec`.
            #[inline]
            pub fn to_vec(self) -> std::vec::Vec<$scalar> {
                vec![$(self.$field),+]
            }
        }

        impl<S: CoordSpace> IntoSpace<S> for $name<S> {
            type Output = $name<S>;
            #[inline]
            fn into_space(self) -> $name<S> { self }
        }
    };
}

define_vec! {
    #[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
    pub struct Vec2i<S>: i32 { x, z }
    ; up = Vec2i { x: 0, z: 1, _space: std::marker::PhantomData }
}

define_vec! {
    #[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
    pub struct Vec3i<S>: i32 { x, y, z }
    ; up = Vec3i { x: 0, y: 1, z: 0, _space: std::marker::PhantomData }
}

define_vec! {
    #[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
    pub struct Vec4i<S>: i32 { x, y, z, w }
}

define_vec! {
    #[derive(Default, Clone, Copy, PartialEq, Serialize, Deserialize, PartialOrd)]
    pub struct Vec2f<S>: f32 { x, z }
    ; up = Vec2f { x: 0.0, z: 1.0, _space: std::marker::PhantomData }
}

define_vec! {
    #[derive(Default, Clone, Copy, PartialEq, Serialize, Deserialize, PartialOrd)]
    pub struct Vec3f<S>: f32 { x, y, z }
    ; up = Vec3f { x: 0.0, y: 1.0, z: 0.0, _space: std::marker::PhantomData }
}

define_vec! {
    #[derive(Default, Clone, Copy, PartialEq, Serialize, Deserialize, PartialOrd)]
    pub struct Vec4f<S>: f32 { x, y, z, w }
}

impl<S: CoordSpace> From<(i32, i32)> for Vec2i<S> {
    #[inline]
    fn from((x, z): (i32, i32)) -> Self {
        Self::new(x, z)
    }
}
impl<S: CoordSpace> From<Vec2i<S>> for (i32, i32) {
    #[inline]
    fn from(v: Vec2i<S>) -> Self {
        (v.x, v.z)
    }
}

impl<S: CoordSpace> From<(i32, i32, i32)> for Vec3i<S> {
    #[inline]
    fn from((x, y, z): (i32, i32, i32)) -> Self {
        Self::new(x, y, z)
    }
}
impl<S: CoordSpace> From<Vec3i<S>> for (i32, i32, i32) {
    #[inline]
    fn from(v: Vec3i<S>) -> Self {
        (v.x, v.y, v.z)
    }
}

impl<S: CoordSpace> From<(i32, i32, i32, i32)> for Vec4i<S> {
    #[inline]
    fn from((x, y, z, w): (i32, i32, i32, i32)) -> Self {
        Self::new(x, y, z, w)
    }
}
impl<S: CoordSpace> From<Vec4i<S>> for (i32, i32, i32, i32) {
    #[inline]
    fn from(v: Vec4i<S>) -> Self {
        (v.x, v.y, v.z, v.w)
    }
}

impl<S: CoordSpace> From<(f32, f32)> for Vec2f<S> {
    #[inline]
    fn from((x, z): (f32, f32)) -> Self {
        Self::new(x, z)
    }
}
impl<S: CoordSpace> From<Vec2f<S>> for (f32, f32) {
    #[inline]
    fn from(v: Vec2f<S>) -> Self {
        (v.x, v.z)
    }
}

impl<S: CoordSpace> From<(f32, f32, f32)> for Vec3f<S> {
    #[inline]
    fn from((x, y, z): (f32, f32, f32)) -> Self {
        Self::new(x, y, z)
    }
}
impl<S: CoordSpace> From<Vec3f<S>> for (f32, f32, f32) {
    #[inline]
    fn from(v: Vec3f<S>) -> Self {
        (v.x, v.y, v.z)
    }
}

impl<S: CoordSpace> From<(f32, f32, f32, f32)> for Vec4f<S> {
    #[inline]
    fn from((x, y, z, w): (f32, f32, f32, f32)) -> Self {
        Self::new(x, y, z, w)
    }
}
impl<S: CoordSpace> From<Vec4f<S>> for (f32, f32, f32, f32) {
    #[inline]
    fn from(v: Vec4f<S>) -> Self {
        (v.x, v.y, v.z, v.w)
    }
}

impl<S: CoordSpace> From<[i32; 2]> for Vec2i<S> {
    #[inline]
    fn from([x, z]: [i32; 2]) -> Self {
        Self::new(x, z)
    }
}
impl<S: CoordSpace> From<Vec2i<S>> for [i32; 2] {
    #[inline]
    fn from(v: Vec2i<S>) -> Self {
        [v.x, v.z]
    }
}

impl<S: CoordSpace> From<[i32; 3]> for Vec3i<S> {
    #[inline]
    fn from([x, y, z]: [i32; 3]) -> Self {
        Self::new(x, y, z)
    }
}
impl<S: CoordSpace> From<Vec3i<S>> for [i32; 3] {
    #[inline]
    fn from(v: Vec3i<S>) -> Self {
        [v.x, v.y, v.z]
    }
}

impl<S: CoordSpace> From<[i32; 4]> for Vec4i<S> {
    #[inline]
    fn from([x, y, z, w]: [i32; 4]) -> Self {
        Self::new(x, y, z, w)
    }
}
impl<S: CoordSpace> From<Vec4i<S>> for [i32; 4] {
    #[inline]
    fn from(v: Vec4i<S>) -> Self {
        [v.x, v.y, v.z, v.w]
    }
}

impl<S: CoordSpace> From<[f32; 2]> for Vec2f<S> {
    #[inline]
    fn from([x, z]: [f32; 2]) -> Self {
        Self::new(x, z)
    }
}
impl<S: CoordSpace> From<Vec2f<S>> for [f32; 2] {
    #[inline]
    fn from(v: Vec2f<S>) -> Self {
        [v.x, v.z]
    }
}

impl<S: CoordSpace> From<[f32; 3]> for Vec3f<S> {
    #[inline]
    fn from([x, y, z]: [f32; 3]) -> Self {
        Self::new(x, y, z)
    }
}
impl<S: CoordSpace> From<Vec3f<S>> for [f32; 3] {
    #[inline]
    fn from(v: Vec3f<S>) -> Self {
        [v.x, v.y, v.z]
    }
}

impl<S: CoordSpace> From<[f32; 4]> for Vec4f<S> {
    #[inline]
    fn from([x, y, z, w]: [f32; 4]) -> Self {
        Self::new(x, y, z, w)
    }
}
impl<S: CoordSpace> From<Vec4f<S>> for [f32; 4] {
    #[inline]
    fn from(v: Vec4f<S>) -> Self {
        [v.x, v.y, v.z, v.w]
    }
}

impl<S: CoordSpace> From<Vec2i<S>> for Vec2f<S> {
    #[inline]
    fn from(v: Vec2i<S>) -> Self {
        Self::new(v.x as f32, v.z as f32)
    }
}
impl<S: CoordSpace> From<Vec3i<S>> for Vec3f<S> {
    #[inline]
    fn from(v: Vec3i<S>) -> Self {
        Self::new(v.x as f32, v.y as f32, v.z as f32)
    }
}
impl<S: CoordSpace> From<Vec4i<S>> for Vec4f<S> {
    #[inline]
    fn from(v: Vec4i<S>) -> Self {
        Self::new(v.x as f32, v.y as f32, v.z as f32, v.w as f32)
    }
}

#[inline(always)]
fn clamp_bound(v: f32, bounds: &impl RangeBounds<f32>) -> f32 {
    let lo = match bounds.start_bound() {
        Bound::Included(&b) => b,
        Bound::Excluded(&b) => b,
        Bound::Unbounded => f32::NEG_INFINITY,
    };
    let hi = match bounds.end_bound() {
        Bound::Included(&b) => b,
        Bound::Excluded(&b) => b,
        Bound::Unbounded => f32::INFINITY,
    };
    v.clamp(lo, hi)
}

impl<S: CoordSpace> Vec2i<S> {
    #[inline]
    pub fn dot(self, rhs: Self) -> i32 {
        self.x * rhs.x + self.z * rhs.z
    }

    #[inline]
    pub fn length_sq(self) -> i32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        (self.length_sq() as f32).sqrt()
    }
}

impl<S: CoordSpace> Vec3i<S> {
    #[inline]
    pub fn dot(self, rhs: Self) -> i32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    #[inline]
    pub fn length_sq(self) -> i32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        (self.length_sq() as f32).sqrt()
    }
}

impl<S: CoordSpace> Vec4i<S> {
    #[inline]
    pub fn dot(self, rhs: Self) -> i32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z + self.w * rhs.w
    }

    #[inline]
    pub fn length_sq(self) -> i32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        (self.length_sq() as f32).sqrt()
    }
}

impl<S: CoordSpace> Vec2f<S> {
    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.z * rhs.z
    }

    #[inline]
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    /// Returns the unit vector, or `ZERO` if the length is zero.
    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len == 0.0 { Self::ZERO } else { self / len }
    }

    /// Clamp each component independently using `RangeBounds`.
    /// Pass `..` for a component to leave it unclamped
    #[inline]
    pub fn clamp(self, bounds: (impl RangeBounds<f32>, impl RangeBounds<f32>)) -> Self {
        Self::new(
            clamp_bound(self.x, &bounds.0),
            clamp_bound(self.z, &bounds.1),
        )
    }
}

impl<S: CoordSpace> Vec3f<S> {
    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    #[inline]
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    /// Returns the unit vector, or `ZERO` if the length is zero.
    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len == 0.0 { Self::ZERO } else { self / len }
    }

    #[inline]
    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    /// Clamp each component independently using `RangeBounds`
    #[inline]
    pub fn clamp(
        self,
        bounds: (
            impl RangeBounds<f32>,
            impl RangeBounds<f32>,
            impl RangeBounds<f32>,
        ),
    ) -> Self {
        Self::new(
            clamp_bound(self.x, &bounds.0),
            clamp_bound(self.y, &bounds.1),
            clamp_bound(self.z, &bounds.2),
        )
    }
}

impl<S: CoordSpace> Vec4f<S> {
    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z + self.w * rhs.w
    }

    #[inline]
    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    /// Returns the unit vector, or `ZERO` if the length is zero.
    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len == 0.0 { Self::ZERO } else { self / len }
    }

    /// Clamp each component independently using `RangeBounds`
    #[inline]
    pub fn clamp(
        self,
        bounds: (
            impl RangeBounds<f32>,
            impl RangeBounds<f32>,
            impl RangeBounds<f32>,
            impl RangeBounds<f32>,
        ),
    ) -> Self {
        Self::new(
            clamp_bound(self.x, &bounds.0),
            clamp_bound(self.y, &bounds.1),
            clamp_bound(self.z, &bounds.2),
            clamp_bound(self.w, &bounds.3),
        )
    }

    /// Column-major 4x4 matrix multiply.
    /// Each `[Vec4f; 4]` is an array of column vectors.
    pub fn mat_mul<S2: CoordSpace>(lhs: [Self; 4], rhs: [Vec4f<S2>; 4]) -> [Self; 4] {
        let mut out = [Self::ZERO; 4];
        for i in 0..4 {
            for j in 0..4 {
                for k in 0..4 {
                    out[i][j] += lhs[k][j] * rhs[i][k];
                }
            }
        }
        out
    }
}

#[inline(always)]
fn floor_div(a: i32, b: i32) -> i32 {
    let d = a / b;
    // subtract 1 when signs differ and there's a remainder
    d - ((a ^ b) >> 31 & (if a % b != 0 { 1 } else { 0 }))
}

#[inline(always)]
fn rem_euclid_i32(a: i32, b: i32) -> i32 {
    ((a % b) + b) % b
}

impl IntoSpace<Local> for Vec3f<Global> {
    type Output = Vec3f<Local>;
    #[inline]
    fn into_space(self) -> Vec3f<Local> {
        Vec3f::new(
            rem_euclid_i32(self.x as i32, CHUNK_SIZE as i32) as f32,
            self.y,
            rem_euclid_i32(self.z as i32, CHUNK_SIZE as i32) as f32,
        )
    }
}
impl IntoSpace<Chunk> for Vec3f<Global> {
    type Output = Vec3f<Chunk>;
    #[inline]
    fn into_space(self) -> Vec3f<Chunk> {
        Vec3f::new(
            floor_div(self.x as i32, CHUNK_SIZE as i32) as f32,
            0.0,
            floor_div(self.z as i32, CHUNK_SIZE as i32) as f32,
        )
    }
}

impl IntoSpace<Local> for Vec3i<Global> {
    type Output = Vec3i<Local>;
    #[inline]
    fn into_space(self) -> Vec3i<Local> {
        Vec3i::new(
            rem_euclid_i32(self.x, CHUNK_SIZE as i32),
            self.y,
            rem_euclid_i32(self.z, CHUNK_SIZE as i32),
        )
    }
}

impl IntoSpace<Chunk> for Vec3i<Global> {
    type Output = Vec3i<Chunk>;
    #[inline]
    fn into_space(self) -> Vec3i<Chunk> {
        Vec3i::new(
            floor_div(self.x, CHUNK_SIZE as i32),
            0,
            floor_div(self.z, CHUNK_SIZE as i32),
        )
    }
}

impl IntoSpace<Chunk> for Vec2i<Global> {
    type Output = Vec2i<Chunk>;
    #[inline]
    fn into_space(self) -> Vec2i<Chunk> {
        Vec2i::new(
            floor_div(self.x, CHUNK_SIZE as i32),
            floor_div(self.z, CHUNK_SIZE as i32),
        )
    }
}

impl IntoSpace<Local> for Vec2i<Global> {
    type Output = Vec2i<Local>;
    #[inline]
    fn into_space(self) -> Vec2i<Local> {
        Vec2i::new(
            rem_euclid_i32(self.x, CHUNK_SIZE as i32),
            rem_euclid_i32(self.z, CHUNK_SIZE as i32),
        )
    }
}

impl IntoSpace<Global> for Vec2i<Chunk> {
    type Output = Vec2i<Global>;
    #[inline]
    fn into_space(self) -> Vec2i<Global> {
        Vec2i::new(self.x * CHUNK_SIZE as i32, self.z * CHUNK_SIZE as i32)
    }
}

/// Convert a local coordinate back to global given the chunk's grid position.
#[inline]
pub fn local_to_global(local: Vec3i<Local>, chunk: Vec2i<Chunk>) -> Vec3i<Global> {
    Vec3i::new(
        chunk.x * CHUNK_SIZE as i32 + local.x,
        local.y,
        chunk.z * CHUNK_SIZE as i32 + local.z,
    )
}

pub type Vec2iGlobal = Vec2i<Global>;
pub type Vec2iLocal = Vec2i<Local>;
pub type Vec2iChunk = Vec2i<Chunk>;

pub type Vec3iGlobal = Vec3i<Global>;
pub type Vec3iLocal = Vec3i<Local>;
pub type Vec3iChunk = Vec3i<Chunk>;

pub type Vec2fGlobal = Vec2f<Global>;
pub type Vec3fGlobal = Vec3f<Global>;

pub const VEC4F_IDENTITY: [Vec4f<Global>; 4] = [
    Vec4f::new(1.0, 0.0, 0.0, 0.0),
    Vec4f::new(0.0, 1.0, 0.0, 0.0),
    Vec4f::new(0.0, 0.0, 1.0, 0.0),
    Vec4f::new(0.0, 0.0, 0.0, 1.0),
];

pub const VEC3F_IDENTITY: [Vec3f<Global>; 3] = [
    Vec3f::new(1.0, 0.0, 0.0),
    Vec3f::new(0.0, 1.0, 0.0),
    Vec3f::new(0.0, 0.0, 1.0),
];

impl<S: CoordSpace> Vec3f<S> {
    pub fn translation_matrix(self) -> [Vec4f<S>; 4] {
        [
            Vec4f::new(1.0, 0.0, 0.0, 0.0),
            Vec4f::new(0.0, 1.0, 0.0, 0.0),
            Vec4f::new(0.0, 0.0, 1.0, 0.0),
            Vec4f::new(self.x(), self.y(), self.z(), 1.0),
        ]
    }
}

impl Vec2iChunk {
    pub fn translation_matrix(self) -> [Vec4f<Chunk>; 4] {
        let chunk_size = CHUNK_SIZE as f32;
        Vec3f::<Chunk>::new(
            self.x() as f32 * chunk_size,
            0.0,
            self.z() as f32 * chunk_size,
        )
        .translation_matrix()
    }
}

impl<S: CoordSpace> From<Vec3f<S>> for Vec2iChunk
where
    Vec3fGlobal: From<Vec3f<S>>,
{
    fn from(value: Vec3f<S>) -> Self {
        let global = Vec3fGlobal::from(value);
        let global = Vec3iGlobal::from([global.x() as i32, global.y() as i32, global.z() as i32]);
        let chunk = IntoSpace::<Chunk>::into_space(global);

        Self::from([chunk.x, chunk.z])
    }
}

impl<S: CoordSpace> From<Vec3i<S>> for Vec2iChunk
where
    Vec3iGlobal: From<Vec3i<S>>,
{
    fn from(value: Vec3i<S>) -> Self {
        let global = Vec3iGlobal::from(value);
        let chunk = IntoSpace::<Chunk>::into_space(global);

        Self::from([chunk.x, chunk.z])
    }
}

impl<S: CoordSpace> From<Vec3f<S>> for Vec3iLocal
where
    Vec3fGlobal: From<Vec3f<S>>,
{
    fn from(value: Vec3f<S>) -> Self {
        let global = Vec3fGlobal::from(value);
        let global = Vec3iGlobal::from([global.x() as i32, global.y() as i32, global.z() as i32]);
        let local = IntoSpace::<Local>::into_space(global);

        Self::from([local.x, local.y, local.z])
    }
}

impl<S: CoordSpace> From<Vec3f<S>> for Vec3i<S> {
    fn from(value: Vec3f<S>) -> Self {
        Self::new(value.x() as i32, value.y() as i32, value.z() as i32)
    }
}

impl Aabb for Vec2iChunk {
    fn aabb<S: CoordSpace>(&self, position: Vec3f<S>) -> AxisAlignedBoundingBox<S> {
        let min = Vec3f::new(
            position.x + self.x as f32 * CHUNK_SIZE as f32,
            position.y,
            position.z + self.z as f32 * CHUNK_SIZE as f32,
        );
        let max = Vec3f::new(
            min.x + CHUNK_SIZE as f32,
            min.y + CHUNK_SIZE as f32,
            min.z + CHUNK_SIZE as f32,
        );
        AxisAlignedBoundingBox::new(min, max)
    }

    fn aabb_swept<S: CoordSpace>(
        &self,
        _position: Vec3f<S>,
        _velocity: Vec3f<S>,
        _dt: std::time::Duration,
    ) -> AxisAlignedBoundingBox<S> {
        unimplemented!("Vec2iChunk cannot have velocity");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic() {
        let a = Vec3iGlobal::new(1, 2, 3);
        let b = Vec3iGlobal::new(4, 5, 6);
        assert_eq!(a + b, Vec3iGlobal::new(5, 7, 9));
        assert_eq!(b - a, Vec3iGlobal::new(3, 3, 3));
        assert_eq!(a * b, Vec3iGlobal::new(4, 10, 18));
        assert_eq!(-a, Vec3iGlobal::new(-1, -2, -3));
    }

    #[test]
    fn scalar_ops() {
        let v = Vec3fGlobal::new(2.0, 4.0, 6.0);
        assert_eq!(v * 2.0, Vec3fGlobal::new(4.0, 8.0, 12.0));
        assert_eq!(v / 2.0, Vec3fGlobal::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn assign_ops() {
        let mut v = Vec3iGlobal::new(1, 2, 3);
        v += Vec3iGlobal::new(10, 10, 10);
        assert_eq!(v, Vec3iGlobal::new(11, 12, 13));
        v *= 2;
        assert_eq!(v, Vec3iGlobal::new(22, 24, 26));
    }

    #[test]
    fn index() {
        let mut v = Vec3iGlobal::new(1, 2, 3);
        assert_eq!(v[0], 1);
        assert_eq!(v[1], 2);
        assert_eq!(v[2], 3);
        v[1] = 99;
        assert_eq!(v.y(), 99);
    }

    #[test]
    fn from_tuple() {
        let v = Vec3iGlobal::from((1, 2, 3));
        assert_eq!(v, Vec3iGlobal::new(1, 2, 3));
        let t: (i32, i32, i32) = v.into();
        assert_eq!(t, (1, 2, 3));
    }

    #[test]
    fn from_array() {
        let v = Vec3iGlobal::from([10, 20, 30]);
        assert_eq!(v[0], 10);
        let a: [i32; 3] = v.into();
        assert_eq!(a, [10, 20, 30]);
    }

    #[test]
    fn global_to_local_positive() {
        // (17, 5, 33) in a chunk size 16 world -> x=1, y=5, z=1
        let g = Vec3i::<Global>::new(17, 5, 33);
        let l = IntoSpace::<Local>::into_space(g);
        assert_eq!(l, Vec3i::new(1, 5, 1));
    }

    #[test]
    fn global_to_local_negative() {
        // Negative coordinates must still give non-negative local values.
        // (-1, 10, -17) -> x=15, y=10, z=15
        let g = Vec3i::<Global>::new(-1, 10, -17);
        let l = IntoSpace::<Local>::into_space(g);
        assert_eq!(l, Vec3i::new(15, 10, 15));
    }

    #[test]
    fn global_to_chunk_positive() {
        // (17, 5, 33) -> chunk (1, 0, 2)
        let g = Vec3i::<Global>::new(17, 5, 33);
        let c = IntoSpace::<Chunk>::into_space(g);
        assert_eq!(c, Vec3i::new(1, 0, 2));
    }

    #[test]
    fn global_to_chunk_negative() {
        // (-1, 0, -17) -> chunk (-1, 0, -2)  (floored, not truncated)
        let g = Vec3i::<Global>::new(-1, 0, -17);
        let c = IntoSpace::<Chunk>::into_space(g);
        assert_eq!(c, Vec3i::new(-1, 0, -2));
    }

    #[test]
    fn roundtrip_global_local_chunk() {
        let g = Vec3i::<Global>::new(37, 64, -5);
        let local: Vec3i<Local> = IntoSpace::<Local>::into_space(g);
        let chunk = IntoSpace::<Chunk>::into_space(g);
        let chunk = Vec2i::<Chunk>::new(chunk.x, chunk.z);
        let back = local_to_global(local, chunk);
        assert_eq!(back, g);
    }

    #[test]
    fn vec2i_global_to_chunk() {
        let g = Vec2i::<Global>::new(-16, 32);
        let c = IntoSpace::<Chunk>::into_space(g);
        assert_eq!(c, Vec2i::new(-1, 2));
    }

    #[test]
    fn vec2i_chunk_to_global_origin() {
        let c = Vec2i::<Chunk>::new(3, -2);
        let g = IntoSpace::<Global>::into_space(c);
        assert_eq!(g, Vec2i::new(48, -32));
    }

    #[test]
    fn identity_conversion() {
        // into_space with the same type should be a no-op.
        let v = Vec3i::<Global>::new(1, 2, 3);
        let same = IntoSpace::<Global>::into_space(v);
        assert_eq!(v, same);
    }

    #[test]
    fn dot_product() {
        let a = Vec3fGlobal::new(1.0, 0.0, 0.0);
        let b = Vec3fGlobal::new(0.0, 1.0, 0.0);
        assert_eq!(a.dot(b), 0.0);
        assert_eq!(a.dot(a), 1.0);

        let c = Vec3fGlobal::new(1.0, 2.0, 3.0);
        let d = Vec3fGlobal::new(4.0, 5.0, 6.0);
        assert_eq!(c.dot(d), 32.0); // 4+10+18
    }

    #[test]
    fn cross_product() {
        let x = Vec3fGlobal::new(1.0, 0.0, 0.0);
        let y = Vec3fGlobal::new(0.0, 1.0, 0.0);
        let z = Vec3fGlobal::new(0.0, 0.0, 1.0);
        assert_eq!(x.cross(y), z);
        assert_eq!(y.cross(x), -z);
    }

    #[test]
    fn length_and_normalize() {
        let v = Vec3fGlobal::new(3.0, 0.0, 4.0);
        assert_eq!(v.length_sq(), 25.0);
        assert_eq!(v.length(), 5.0);

        let n = v.normalize();
        assert!((n.length() - 1.0).abs() < 1e-6);

        assert_eq!(Vec3fGlobal::ZERO.normalize(), Vec3fGlobal::ZERO);
    }

    #[test]
    fn vec4f_dot_and_length() {
        let v = Vec4f::<Global>::new(1.0, 0.0, 0.0, 0.0);
        assert_eq!(v.dot(v), 1.0);
        assert_eq!(v.length(), 1.0);
    }

    #[test]
    fn vec2f_dot_normalize() {
        let v = Vec2fGlobal::new(3.0, 4.0);
        assert_eq!(v.length(), 5.0);
        let n = v.normalize();
        assert!((n.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn clamp_vec3f() {
        let v = Vec3fGlobal::new(-2.0, 5.0, 0.5);

        // clamp x to [-1, 1], leave y and z unbounded
        let c = v.clamp((-1.0..=1.0, .., ..));
        assert_eq!(c.x(), -1.0);
        assert_eq!(c.y(), 5.0);
        assert_eq!(c.z(), 0.5);

        // clamp y to [0, 3], z to [0..]
        let c2 = v.clamp((.., 0.0..=3.0, 0.0..));
        assert_eq!(c2.x(), -2.0);
        assert_eq!(c2.y(), 3.0);
        assert_eq!(c2.z(), 0.5);
    }

    #[test]
    fn clamp_vec2f() {
        let v = Vec2fGlobal::new(-5.0, 10.0);
        let c = v.clamp((0.0.., ..=8.0));
        assert_eq!(c.x(), 0.0);
        assert_eq!(c.z(), 8.0);
    }

    #[test]
    fn clamp_vec4f() {
        let v = Vec4f::<Global>::new(2.0, -2.0, 0.5, 100.0);
        let c = v.clamp((.., -1.0..=1.0, .., 0.0..=1.0));
        assert_eq!(c.x(), 2.0);
        assert_eq!(c.y(), -1.0);
        assert_eq!(c.z(), 0.5);
        assert_eq!(c.w(), 1.0);
    }

    #[test]
    fn mat_mul_identity() {
        let identity: [Vec4f<Global>; 4] = [
            Vec4f::new(1.0, 0.0, 0.0, 0.0),
            Vec4f::new(0.0, 1.0, 0.0, 0.0),
            Vec4f::new(0.0, 0.0, 1.0, 0.0),
            Vec4f::new(0.0, 0.0, 0.0, 1.0),
        ];
        let m: [Vec4f<Global>; 4] = [
            Vec4f::new(1.0, 2.0, 3.0, 4.0),
            Vec4f::new(5.0, 6.0, 7.0, 8.0),
            Vec4f::new(9.0, 10.0, 11.0, 12.0),
            Vec4f::new(13.0, 14.0, 15.0, 16.0),
        ];
        let result = Vec4f::mat_mul(m, identity);
        for i in 0..4 {
            assert_eq!(result[i], m[i]);
        }
    }

    #[test]
    fn chunk_local_from_player_global() {
        let player_global = Vec3fGlobal::new(17.0, 5.0, 33.0);
        let chunk_local = Vec2iChunk::from(player_global);
        assert_eq!(chunk_local, Vec2iChunk::new(1, 2));

        let player_global = Vec3fGlobal::new(0.0, 5.0, 0.0);
        let chunk_local = Vec2iChunk::from(player_global);
        assert_eq!(chunk_local, Vec2iChunk::new(0, 0));
    }

    #[test]
    fn distance_length() {
        let a = Vec2iChunk::new(0, 0);
        let b = Vec2iChunk::new(3, 4);
        assert_eq!((b - a).length_sq(), 25);
        assert_eq!((b - a).length(), 5.0);
    }
}
