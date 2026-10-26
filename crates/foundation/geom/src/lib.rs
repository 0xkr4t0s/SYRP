//! 2D geometry for Syrp with JUCE-equivalent semantics.
//!
//! This first increment provides [`Point`], [`Rect`] (generic over the [`Coord`] scalar types
//! `i32`, `f32` and `f64`). Coordinates follow screen conventions: x grows to the right and
//! y grows downwards.
//!
//! ```
//! use syrp_geom::{Point, Rect};
//! let mut area = Rect::new(0, 0, 400, 300);
//! let header = area.take_top(40);
//! assert_eq!(header, Rect::new(0, 0, 400, 40));
//! assert!(area.contains_point(Point::new(0, 40)));
//! ```

mod coord;
mod point;
mod rect;

pub use coord::Coord;
pub use point::Point;
pub use rect::Rect;
