//! 2D geometry for Syrp with JUCE-equivalent semantics.
//!
//! This crate provides [`Point`], [`Rect`], [`RectList`], [`Line`], [`Parallelogram`] and
//! [`BorderSize`] (generic over the [`Coord`] scalar types `i32`, `f32` and `f64`),
//! [`AffineTransform`], and the alignment types [`Justification`] and [`RectanglePlacement`].
//! Coordinates follow screen conventions: x grows to the right and y grows downwards.
//!
//! The `kurbo` feature adds `From` conversions between the `f64` types and their `kurbo`
//! equivalents.
//!
//! ```
//! use syrp_geom::{AffineTransform, Point, Rect};
//! let mut area = Rect::new(0, 0, 400, 300);
//! let header = area.take_top(40);
//! assert_eq!(header, Rect::new(0, 0, 400, 40));
//! let p = AffineTransform::translation(5.0, 5.0).transform_point(Point::new(1.0, 1.0));
//! assert_eq!(p, Point::new(6.0, 6.0));
//! ```

mod border;
mod coord;
#[cfg(feature = "kurbo")]
mod kurbo_interop;
mod line;
mod placement;
mod point;
mod rect;
mod rect_list;
mod transform;

pub use border::BorderSize;
pub use coord::Coord;
pub use line::{Line, Parallelogram};
pub use placement::{Justification, RectanglePlacement};
pub use point::Point;
pub use rect::Rect;
pub use rect_list::RectList;
pub use transform::AffineTransform;
