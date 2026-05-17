//! 3D Projective Geometric Algebra integrated with units of measurement
//!
//! All multivector functions are expressed foremost as traits. However, for convenience, the following operators are overloaded for multivectors:
//! - `*`: `GeometricProduct`
//! - `|`: `InnerProduct`
//! - `^`: `OuterProduct`
//! - `&`: `RegressiveProduct`
//! - `%`: `CommutatorProduct`
//! - `!`: `Reverse`
//! - `>>`: `SandwichProduct` for `Motor`s, and the negative `SandwichProduct` for `Flector`s
//!
//! Be mindful of operator precedence when using these overloads. Additionally, due to the orphan rule, these overloads do not support scalars on the left-hand side.

pub use uom;

use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Neg, Sub};
use uom::si::{
    self,
    f64::{Angle, Length, Ratio},
};
use uom::typenum::Prod;

type Quantity<D> = uom::si::Quantity<D, uom::si::SI<f64>, f64>;

/// Trait for values that can be components in multivectors
/// Due to the properties of e0, the full requirement for T to be a component of a multivector are:
/// - `T: Component + Mul<Length>`
/// - `E0<T>: Component`
pub trait Component:
    Copy + Sized + Default + Debug + Add<Output = Self> + Sub<Output = Self> + Neg<Output = Self>
{
}

impl<D: si::Dimension + ?Sized> Component for Quantity<D> where
    Self: Copy
        + Sized
        + Default
        + Debug
        + Add<Output = Self>
        + Sub<Output = Self>
        + Neg<Output = Self>
{
}
impl Component for f64 {}

/// Given a multivector component type `T`, `E0<T>` is the type of coefficients for bases containing `e0`. This is obtained by multiplying the dimension by length.
pub type E0<T> = Prod<T, Length>;
type Scalar<T> = T;

mod generated {
    #![allow(unused_parens)]
    #![allow(clippy::suspicious_arithmetic_impl)]
    #![allow(clippy::used_underscore_binding)]
    #![allow(clippy::no_effect_underscore_binding)]
    #![allow(clippy::unnecessary_struct_initialization)]
    #![allow(clippy::default_trait_access)]
    use super::{Component, ConvertValues, E0, Norm, Quantity, Reverse, Scalar};
    use std::{
        fmt::Debug,
        ops::{Add, Mul},
    };
    use uom::{
        si::{
            self,
            f64::{Length, LinearNumberDensity, Ratio},
        },
        typenum::Prod,
    };
    include!(concat!(env!("OUT_DIR"), "/ga_impls.rs"));
}
pub use generated::*;

/// Convert between a multivector and an array of its components, given conversion factors
pub trait ConvertValues<T>
where
    T: Component + Mul<Length>,
    E0<T>: Component,
{
    type Values;
    #[must_use]
    fn into_values(self, unit: T, ideal_unit: E0<T>) -> Self::Values;
    #[must_use]
    fn from_values(values: Self::Values, unit: T, ideal_unit: E0<T>) -> Self;
}

pub trait Reverse {
    #[must_use]
    fn reverse(self) -> Self;
}

pub trait Norm
where
    Self::Output: Mul,
{
    type Output;
    #[must_use]
    fn norm(self) -> Self::Output;
    #[must_use]
    fn normsq(self) -> Prod<Self::Output, Self::Output>;
}

pub trait Normalize {
    type Output;
    #[must_use]
    fn normalized(self) -> Self::Output;
}
impl<T: Sized + Copy + Norm + Div<<T as Norm>::Output>> Normalize for T {
    type Output = <Self as Div<<Self as Norm>::Output>>::Output;
    fn normalized(self) -> Self::Output {
        self / self.norm()
    }
}

pub trait SandwichProduct<Rhs> {
    type Output;
    #[must_use]
    fn sandwich_product(self, rhs: Rhs) -> Self::Output;
}

impl<M, X> SandwichProduct<X> for M
where
    M: Copy + Reverse + GeometricProduct<X>,
    <M as GeometricProduct<X>>::Output: GeometricProduct<M>,
    <<M as GeometricProduct<X>>::Output as GeometricProduct<M>>::Output: Into<X>,
{
    type Output = X;
    fn sandwich_product(self, rhs: X) -> Self::Output {
        self.geometric_product(rhs)
            .geometric_product(self.reverse())
            .into()
    }
}
impl<T: Component + Mul<Length>, X> std::ops::Shr<X> for Motor<T>
where
    E0<T>: Component,
    Self: SandwichProduct<X, Output = X>,
{
    type Output = X;
    fn shr(self, rhs: X) -> Self::Output {
        self.sandwich_product(rhs)
    }
}
impl<T: Component + Mul<Length>, X> std::ops::Shr<X> for Flector<T>
where
    E0<T>: Component,
    Self: SandwichProduct<X, Output = X>,
    X: Neg<Output = X>,
{
    type Output = X;
    fn shr(self, rhs: X) -> Self::Output {
        -self.sandwich_product(rhs)
    }
}

pub trait Exponential {
    type Output;
    #[must_use]
    fn exp(self) -> Self::Output;
}

impl<T> Exponential for Bivector<T>
where
    T: Component + Mul<Length> + Into<Ratio>,
    E0<T>: Component + Into<Length>,
{
    type Output = Motor<Ratio>;

    // Formula from https://bivector.net/PGAdyn.pdf, page 18
    fn exp(self) -> Self::Output {
        let bv = Bivector::<Ratio> {
            e01: self.e01.into(),
            e02: self.e02.into(),
            e03: self.e03.into(),
            e12: self.e12.into(),
            e13: self.e13.into(),
            e23: self.e23.into(),
        };
        let normsq = bv.normsq();
        if normsq == Ratio::from(0.0) {
            return Motor {
                e: 1.0.into(),
                e01: bv.e01,
                e02: bv.e02,
                e03: bv.e03,
                ..Default::default()
            };
        }
        let m = bv.e01 * bv.e23 - bv.e02 * bv.e13 + bv.e03 * bv.e12;
        let angle: Angle = normsq.sqrt().into();
        let cos = angle.cos();
        let sinc = angle.sin() / angle;
        let t = m / normsq * (cos - sinc);
        Motor {
            e: cos,
            e01: sinc * bv.e01 + t * bv.e23,
            e02: sinc * bv.e02 - t * bv.e13,
            e03: sinc * bv.e03 + t * bv.e12,
            e12: sinc * bv.e12,
            e13: sinc * bv.e13,
            e23: sinc * bv.e23,
            e0123: m * sinc,
        }
    }
}

/*
trait Dual {
    type Output;
    fn dual(self, length_unit: Length) -> Self::Output;
}

impl<T: Component + Mul<Length>> Dual for Vector<T>
where
    E0<T>: Component,
    E0<T>: Div<Length, Output = T>,
{
    type Output = Trivector<T>;
    fn dual(self, length_unit: Length) -> Self::Output {
        Trivector {
            e012: -self.e3 * length_unit,
            e013: self.e2 * length_unit,
            e023: -self.e1 * length_unit,
            e123: self.e0 / length_unit,
        }
    }
}

impl<T: Component + Mul<Length>> Dual for Bivector<T>
where
    E0<T>: Component,
    E0<T>: Div<Length, Output = T>,
{
    type Output = Bivector<T>;
    fn dual(self, length_unit: Length) -> Self::Output {
        Bivector {
            e01: self.e23 * length_unit,
            e02: -self.e13 * length_unit,
            e03: self.e12 * length_unit,
            e12: self.e03 / length_unit,
            e13: -self.e02 / length_unit,
            e23: self.e01 / length_unit,
        }
    }
}
*/

impl Motor<Ratio> {
    /// The identity motor, representing no transformation
    #[inline]
    #[must_use]
    pub fn id() -> Self {
        Self {
            e: 1.0.into(),
            ..Default::default()
        }
    }
}
