// SPDX-License-Identifier: MIT OR Apache-2.0
//! Display enumeration: each display's bounds, work area and scale factor.

use syrp_geom::Rect;

/// One display (monitor), as [`Desktop::displays`](crate::Desktop::displays) reports it.
/// Bounds and work area are logical units in desktop coordinates; the work area excludes
/// task bars, docks and menu bars.
/// ```
/// use syrp_geom::Rect;
/// use syrp_peer_api::Display;
/// let d = Display::new(Rect::new(0.0, 0.0, 1440.0, 900.0), Rect::new(0.0, 25.0, 1440.0, 875.0), 2.0)
///     .with_primary(true);
/// assert_eq!((d.scale(), d.is_primary(), d.work_area().y()), (2.0, true, 25.0));
/// ```
#[doc(alias = "Displays::Display")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    bounds: Rect<f64>,
    work_area: Rect<f64>,
    scale: f64,
    primary: bool,
}

impl Display {
    /// A non-primary display. A `scale` that is not finite and positive becomes 1.0, so a
    /// display is always usable with `RenderTarget`.
    /// ```
    /// # use syrp_geom::Rect; use syrp_peer_api::Display;
    /// let r = Rect::new(0.0, 0.0, 800.0, 600.0);
    /// assert_eq!(Display::new(r, r, f64::NAN).scale(), 1.0);
    /// ```
    pub fn new(bounds: Rect<f64>, work_area: Rect<f64>, scale: f64) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        Display {
            bounds,
            work_area,
            scale,
            primary: false,
        }
    }

    /// Marks the display as the primary one (the one with the main menu bar or task bar).
    /// ```
    /// # use syrp_geom::Rect; use syrp_peer_api::Display;
    /// assert!(Display::new(Rect::default(), Rect::default(), 1.0).with_primary(true).is_primary());
    /// ```
    #[must_use]
    pub fn with_primary(mut self, primary: bool) -> Self {
        self.primary = primary;
        self
    }

    /// The whole display in logical desktop coordinates.
    /// ```
    /// # use syrp_geom::Rect; use syrp_peer_api::Display;
    /// let r = Rect::new(1920.0, 0.0, 1280.0, 1024.0);
    /// assert_eq!(Display::new(r, r, 1.0).bounds(), r);
    /// ```
    pub fn bounds(&self) -> Rect<f64> {
        self.bounds
    }

    /// The part of the display windows should normally occupy.
    /// ```
    /// # use syrp_geom::Rect; use syrp_peer_api::Display;
    /// let w = Rect::new(0.0, 0.0, 800.0, 560.0);
    /// assert_eq!(Display::new(Rect::new(0.0, 0.0, 800.0, 600.0), w, 1.0).work_area(), w);
    /// ```
    pub fn work_area(&self) -> Rect<f64> {
        self.work_area
    }

    /// Physical pixels per logical unit.
    /// ```
    /// # use syrp_geom::Rect; use syrp_peer_api::Display;
    /// assert_eq!(Display::new(Rect::default(), Rect::default(), 1.5).scale(), 1.5);
    /// ```
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Whether this is the primary display.
    /// ```
    /// # use syrp_geom::Rect; use syrp_peer_api::Display;
    /// assert!(!Display::new(Rect::default(), Rect::default(), 1.0).is_primary());
    /// ```
    pub fn is_primary(&self) -> bool {
        self.primary
    }
}
