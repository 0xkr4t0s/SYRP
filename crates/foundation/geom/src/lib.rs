//! 2D geometry for Syrp with JUCE-equivalent semantics.
//!
//! This crate provides [`Point`], [`Rect`] (generic over the [`Coord`] scalar types
//! `i32`, `f32` and `f64`) and [`AffineTransform`]. Coordinates follow screen conventions:
//! x grows to the right and y grows downwards.
//!
//! ```
//! use syrp_geom::{AffineTransform, Point, Rect};
//! let mut area = Rect::new(0, 0, 400, 300);
//! let header = area.take_top(40);
//! assert_eq!(header, Rect::new(0, 0, 400, 40));
//! let p = AffineTransform::translation(5.0, 5.0).transform_point(Point::new(1.0, 1.0));
//! assert_eq!(p, Point::new(6.0, 6.0));
//! ```

mod coord;
mod point;
mod rect;
mod transform;

pub use coord::Coord;
pub use point::Point;
pub use rect::Rect;
pub use transform::AffineTransform;
