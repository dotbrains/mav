//! The GPUI geometry module is a collection of types and traits that
//! can be used to describe common units, concepts, and the relationships
//! between them.

use anyhow::{Context as _, anyhow};
use core::fmt::Debug;
use derive_more::{Add, AddAssign, Div, DivAssign, Neg, Sub, SubAssign};
use refineable::Refineable;
use schemars::{JsonSchema, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::borrow::Cow;
use std::ops::Range;
use std::{
    cmp::{self, PartialOrd},
    fmt::{self, Display},
    hash::Hash,
    ops::{Add, Div, Mul, MulAssign, Neg, Sub},
};
use taffy::prelude::{TaffyGridLine, TaffyGridSpan};

use crate::{App, DisplayId};

mod anchor;
mod axis;
mod bounds_access;
mod bounds_core;
mod bounds_ops;
mod corners;
mod edges;
mod grid;
mod length;
mod pixel_conversions;
#[path = "geometry/pixel_units.rs"]
mod pixel_units;
mod point;
#[path = "geometry/scaled_units.rs"]
mod scaled_units;
mod size;
mod traits;

pub use anchor::*;
pub use axis::*;
pub use bounds_core::*;
pub use corners::*;
pub use edges::*;
pub use grid::*;
pub use length::*;
pub use pixel_units::*;
pub use point::*;
pub use scaled_units::*;
pub use size::*;
pub use traits::*;

#[cfg(test)]
mod tests;
