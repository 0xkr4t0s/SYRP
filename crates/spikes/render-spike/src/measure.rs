// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Determinism runs (spec B2) and per-scene timing (spec B3).

use std::time::{Duration, Instant};

use syrp_gfx_api::{RenderError, RenderTarget, Renderer};

use crate::Scene;

/// A 64-bit FNV-1a hash of a target's pixel bytes (0 for a target without pixels): equal
/// fingerprints stand for byte-identical renders in determinism reports.
/// ```
/// let t = syrp_gfx_api::RenderTarget::with_pixels(2, 2, 1.0)?;
/// assert_eq!(syrp_render_spike::fingerprint(&t), syrp_render_spike::fingerprint(&t.clone()));
/// # Ok::<(), syrp_gfx_api::RenderError>(())
/// ```
pub fn fingerprint(target: &RenderTarget) -> u64 {
    let bytes = target.pixels().unwrap_or_default();
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The outcome of [`check_determinism`]: how many distinct outputs `runs` renders produced.
/// ```
/// use syrp_render_spike::{Backend, check_determinism, scenes};
/// let d = check_determinism(|| Backend::TinySkia.renderer(), &scenes()[0], 1.0, 3)?;
/// assert_eq!((d.runs(), d.distinct_outputs(), d.is_deterministic()), (3, 1, true));
/// # Ok::<(), syrp_gfx_api::RenderError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Determinism {
    fingerprints: Vec<u64>,
}

impl Determinism {
    /// Renders performed.
    /// ```
    /// # use syrp_render_spike::*;
    /// let d = check_determinism(|| Backend::VelloCpu.renderer(), &scenes()[1], 1.0, 2)?;
    /// assert_eq!(d.runs(), 2);
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn runs(&self) -> usize {
        self.fingerprints.len()
    }

    /// Distinct output byte strings among the runs (1 when deterministic).
    /// ```
    /// # use syrp_render_spike::*;
    /// let d = check_determinism(|| Backend::VelloCpu.renderer(), &scenes()[1], 1.0, 2)?;
    /// assert_eq!(d.distinct_outputs(), 1);
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn distinct_outputs(&self) -> usize {
        let mut f = self.fingerprints.clone();
        f.sort_unstable();
        f.dedup();
        f.len()
    }

    /// Whether every run produced the same bytes.
    /// ```
    /// # use syrp_render_spike::*;
    /// let d = check_determinism(|| Backend::TinySkia.renderer(), &scenes()[1], 2.0, 2)?;
    /// assert!(d.is_deterministic());
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn is_deterministic(&self) -> bool {
        self.distinct_outputs() <= 1
    }

    /// The first run's [`fingerprint`] (0 when no run was made).
    /// ```
    /// # use syrp_render_spike::*;
    /// let d = check_determinism(|| Backend::TinySkia.renderer(), &scenes()[1], 1.0, 1)?;
    /// assert_ne!(d.fingerprint(), 0);
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn fingerprint(&self) -> u64 {
        self.fingerprints.first().copied().unwrap_or(0)
    }
}

/// Renders `scene` at `scale` `runs` times, each with a fresh renderer from `make` into a fresh
/// target, and records each output's [`fingerprint`].
///
/// # Errors
/// The first [`RenderError`] from the target or the renderer.
/// ```
/// use syrp_render_spike::{Backend, check_determinism, scenes};
/// let d = check_determinism(|| Backend::VelloCpu.renderer(), &scenes()[7], 1.25, 4)?;
/// assert!(d.is_deterministic());
/// # Ok::<(), syrp_gfx_api::RenderError>(())
/// ```
pub fn check_determinism(
    mut make: impl FnMut() -> Box<dyn Renderer>,
    scene: &Scene,
    scale: f64,
    runs: usize,
) -> Result<Determinism, RenderError> {
    let mut fingerprints = Vec::with_capacity(runs.min(1024));
    for _ in 0..runs {
        let mut target = scene.target(scale)?;
        make().render(scene.list(), &mut target)?;
        fingerprints.push(fingerprint(&target));
    }
    Ok(Determinism { fingerprints })
}

/// Wall-clock render times from [`time_scene`], sorted fastest first.
/// ```
/// use syrp_render_spike::{Backend, scenes, time_scene};
/// let t = time_scene(Backend::TinySkia.renderer().as_mut(), &scenes()[0], 1.0, 3)?;
/// assert_eq!(t.runs(), 3);
/// assert!(t.min() <= t.median());
/// # Ok::<(), syrp_gfx_api::RenderError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timing {
    samples: Vec<Duration>,
}

impl Timing {
    /// Timed renders.
    /// ```
    /// # use syrp_render_spike::*;
    /// let t = time_scene(Backend::VelloCpu.renderer().as_mut(), &scenes()[0], 1.0, 2)?;
    /// assert_eq!(t.runs(), 2);
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn runs(&self) -> usize {
        self.samples.len()
    }

    /// The fastest render (zero when no run was made).
    /// ```
    /// # use syrp_render_spike::*;
    /// let t = time_scene(Backend::VelloCpu.renderer().as_mut(), &scenes()[0], 1.0, 2)?;
    /// assert!(t.min() > std::time::Duration::ZERO);
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn min(&self) -> Duration {
        self.samples.first().copied().unwrap_or_default()
    }

    /// The median render (the upper median for an even count; zero when no run was made).
    /// ```
    /// # use syrp_render_spike::*;
    /// let t = time_scene(Backend::TinySkia.renderer().as_mut(), &scenes()[0], 1.0, 3)?;
    /// assert!(t.median() >= t.min());
    /// # Ok::<(), syrp_gfx_api::RenderError>(())
    /// ```
    pub fn median(&self) -> Duration {
        self.samples
            .get(self.samples.len() / 2)
            .copied()
            .unwrap_or_default()
    }
}

/// Times `runs` renders of `scene` at `scale` with one `renderer`, after one untimed warm-up
/// render. One target is allocated up front and cleared, untimed, before each run, so a timed
/// render covers list replay and rasterisation only.
///
/// # Errors
/// The first [`RenderError`] from the target or the renderer.
/// ```
/// use syrp_render_spike::{Backend, scenes, time_scene};
/// let t = time_scene(Backend::VelloCpu.renderer().as_mut(), &scenes()[3], 2.0, 3)?;
/// assert!(t.median() > std::time::Duration::ZERO);
/// # Ok::<(), syrp_gfx_api::RenderError>(())
/// ```
pub fn time_scene(
    renderer: &mut dyn Renderer,
    scene: &Scene,
    scale: f64,
    runs: usize,
) -> Result<Timing, RenderError> {
    let mut target = scene.target(scale)?;
    renderer.render(scene.list(), &mut target)?;
    let mut samples = Vec::with_capacity(runs.min(1024));
    for _ in 0..runs {
        target
            .pixels_mut()
            .ok_or(RenderError::InvalidTarget)?
            .fill(0);
        let start = Instant::now();
        renderer.render(scene.list(), &mut target)?;
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    Ok(Timing { samples })
}
