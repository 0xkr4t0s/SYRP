// SPDX-License-Identifier: MIT OR Apache-2.0
//! Colours, gradient stops and brushes.

use syrp_geom::Point;

use crate::path::non_neg;

/// An 8-bit-per-channel colour with straight (not premultiplied) alpha, stored as ARGB.
/// ```
/// use syrp_gfx_api::Colour;
/// let c = Colour::from_rgba(0x11, 0x22, 0x33, 0x80);
/// assert_eq!(c.argb(), 0x8011_2233);
/// assert_eq!(c.to_rgba(), [0x11, 0x22, 0x33, 0x80]);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Colour(u32);

impl Colour {
    /// Fully transparent black (the default).
    pub const TRANSPARENT: Colour = Colour(0);
    /// Opaque black.
    pub const BLACK: Colour = Colour(0xff00_0000);
    /// Opaque white.
    pub const WHITE: Colour = Colour(0xffff_ffff);

    /// From a packed `0xAARRGGBB` value.
    /// ```
    /// assert_eq!(syrp_gfx_api::Colour::from_argb(0xff00_0000), syrp_gfx_api::Colour::BLACK);
    /// ```
    pub const fn from_argb(argb: u32) -> Self {
        Colour(argb)
    }

    /// From red, green, blue and alpha bytes.
    /// ```
    /// assert_eq!(syrp_gfx_api::Colour::from_rgba(255, 255, 255, 255), syrp_gfx_api::Colour::WHITE);
    /// ```
    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Colour(u32::from_be_bytes([a, r, g, b]))
    }

    /// The packed `0xAARRGGBB` value.
    /// ```
    /// assert_eq!(syrp_gfx_api::Colour::WHITE.argb(), 0xffff_ffff);
    /// ```
    #[doc(alias = "getARGB")]
    pub const fn argb(self) -> u32 {
        self.0
    }

    /// The channels as `[r, g, b, a]`.
    /// ```
    /// assert_eq!(syrp_gfx_api::Colour::from_argb(0x8011_2233).to_rgba(), [0x11, 0x22, 0x33, 0x80]);
    /// ```
    pub const fn to_rgba(self) -> [u8; 4] {
        let [a, r, g, b] = self.0.to_be_bytes();
        [r, g, b, a]
    }
}

/// A colour at a position along a gradient. Offsets are clamped to `[0, 1]`; NaN becomes 0.
/// ```
/// use syrp_gfx_api::{Colour, GradientStop};
/// assert_eq!(GradientStop::new(1.5, Colour::WHITE).offset(), 1.0);
/// ```
#[doc(alias = "addColour")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    offset: f32,
    colour: Colour,
}

impl GradientStop {
    /// A stop at `offset` (clamped to `[0, 1]`).
    /// ```
    /// let s = syrp_gfx_api::GradientStop::new(f32::NAN, syrp_gfx_api::Colour::BLACK);
    /// assert_eq!(s.offset(), 0.0);
    /// ```
    pub fn new(offset: f32, colour: Colour) -> Self {
        GradientStop {
            offset: non_neg(offset).min(1.0),
            colour,
        }
    }

    /// Position along the gradient, in `[0, 1]`.
    /// ```
    /// assert_eq!(syrp_gfx_api::GradientStop::new(0.25, Default::default()).offset(), 0.25);
    /// ```
    pub fn offset(self) -> f32 {
        self.offset
    }

    /// The stop's colour.
    /// ```
    /// use syrp_gfx_api::{Colour, GradientStop};
    /// assert_eq!(GradientStop::new(0.0, Colour::WHITE).colour(), Colour::WHITE);
    /// ```
    pub fn colour(self) -> Colour {
        self.colour
    }
}

/// What a brush paints, without its gradient stops. Radii are clamped to `>= 0`.
/// ```
/// use syrp_gfx_api::{Brush, BrushKind, Colour};
/// assert_eq!(Brush::solid(Colour::BLACK).as_view().kind(), BrushKind::Solid(Colour::BLACK));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum BrushKind {
    /// One colour everywhere.
    Solid(Colour),
    /// Colour varies along the line from `start` (offset 0) to `end` (offset 1).
    #[non_exhaustive]
    Linear {
        /// Point at offset 0.
        start: Point<f32>,
        /// Point at offset 1.
        end: Point<f32>,
    },
    /// Colour varies with distance from `centre`, reaching offset 1 at `radius`.
    #[non_exhaustive]
    Radial {
        /// Centre (offset 0).
        centre: Point<f32>,
        /// Distance at offset 1.
        radius: f32,
    },
    /// Colour varies between two circles (offset 0 on the first, 1 on the second).
    #[non_exhaustive]
    TwoPointConical {
        /// First circle's centre.
        start: Point<f32>,
        /// First circle's radius.
        start_radius: f32,
        /// Second circle's centre.
        end: Point<f32>,
        /// Second circle's radius.
        end_radius: f32,
    },
}

/// A solid colour or a gradient (JUCE's FillType and ColourGradient). Gradient stops are kept
/// sorted by offset (stable, so equal offsets keep their order, giving a hard edge). A gradient
/// with no stops paints nothing; with one stop it paints that colour.
/// ```
/// use syrp_gfx_api::{Brush, Colour, GradientStop};
/// use syrp_geom::Point;
/// let b = Brush::linear(Point::new(0.0, 0.0), Point::new(100.0, 0.0), [
///     GradientStop::new(1.0, Colour::WHITE),
///     GradientStop::new(0.0, Colour::BLACK),
/// ]);
/// assert_eq!(b.as_view().stops()[0].colour(), Colour::BLACK);
/// ```
#[doc(alias = "FillType")]
#[doc(alias = "ColourGradient")]
#[derive(Clone, Debug, PartialEq)]
pub struct Brush {
    kind: BrushKind,
    stops: Vec<GradientStop>,
}

impl Brush {
    /// A solid colour.
    /// ```
    /// let _ = syrp_gfx_api::Brush::solid(syrp_gfx_api::Colour::WHITE);
    /// ```
    pub fn solid(colour: Colour) -> Self {
        Brush {
            kind: BrushKind::Solid(colour),
            stops: Vec::new(),
        }
    }

    /// A linear gradient from `start` to `end`.
    /// ```
    /// use syrp_geom::Point;
    /// let _ = syrp_gfx_api::Brush::linear(Point::new(0.0, 0.0), Point::new(1.0, 0.0), []);
    /// ```
    pub fn linear(
        start: Point<f32>,
        end: Point<f32>,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Self {
        Self::gradient(BrushKind::Linear { start, end }, stops)
    }

    /// A radial gradient around `centre`.
    /// ```
    /// use syrp_geom::Point;
    /// let _ = syrp_gfx_api::Brush::radial(Point::new(0.0, 0.0), 5.0, []);
    /// ```
    pub fn radial(
        centre: Point<f32>,
        radius: f32,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Self {
        Self::gradient(
            BrushKind::Radial {
                centre,
                radius: non_neg(radius),
            },
            stops,
        )
    }

    /// A two-point conical gradient between two circles.
    /// ```
    /// use syrp_geom::Point;
    /// let _ = syrp_gfx_api::Brush::two_point_conical(Point::new(0.0, 0.0), 0.0, Point::new(4.0, 0.0), 8.0, []);
    /// ```
    pub fn two_point_conical(
        start: Point<f32>,
        start_radius: f32,
        end: Point<f32>,
        end_radius: f32,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Self {
        let (start_radius, end_radius) = (non_neg(start_radius), non_neg(end_radius));
        Self::gradient(
            BrushKind::TwoPointConical {
                start,
                start_radius,
                end,
                end_radius,
            },
            stops,
        )
    }

    fn gradient(kind: BrushKind, stops: impl IntoIterator<Item = GradientStop>) -> Self {
        let mut stops: Vec<_> = stops.into_iter().collect();
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        Brush { kind, stops }
    }

    /// A borrowed view of this brush.
    /// ```
    /// assert!(syrp_gfx_api::Brush::solid(Default::default()).as_view().stops().is_empty());
    /// ```
    pub fn as_view(&self) -> BrushRef<'_> {
        BrushRef::new(self.kind, &self.stops)
    }
}

impl From<Colour> for Brush {
    fn from(colour: Colour) -> Self {
        Brush::solid(colour)
    }
}

/// A borrowed [`Brush`], as read back from a `DisplayList`. A [`Colour`] converts into one
/// without allocating.
/// ```
/// use syrp_gfx_api::{BrushKind, BrushRef, Colour};
/// let b = BrushRef::from(Colour::WHITE);
/// assert_eq!(b.kind(), BrushKind::Solid(Colour::WHITE));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushRef<'a> {
    kind: BrushKind,
    stops: &'a [GradientStop],
}

impl<'a> BrushRef<'a> {
    pub(crate) fn new(kind: BrushKind, stops: &'a [GradientStop]) -> Self {
        BrushRef { kind, stops }
    }

    /// What the brush paints.
    /// ```
    /// use syrp_gfx_api::{BrushKind, BrushRef, Colour};
    /// assert!(matches!(BrushRef::from(Colour::BLACK).kind(), BrushKind::Solid(_)));
    /// ```
    pub fn kind(self) -> BrushKind {
        self.kind
    }

    /// The gradient stops, sorted by offset (empty for a solid brush).
    /// ```
    /// assert!(syrp_gfx_api::BrushRef::from(syrp_gfx_api::Colour::BLACK).stops().is_empty());
    /// ```
    pub fn stops(self) -> &'a [GradientStop] {
        self.stops
    }
}

impl From<Colour> for BrushRef<'static> {
    fn from(colour: Colour) -> Self {
        BrushRef::new(BrushKind::Solid(colour), &[])
    }
}

impl<'a> From<&'a Brush> for BrushRef<'a> {
    fn from(brush: &'a Brush) -> Self {
        brush.as_view()
    }
}
