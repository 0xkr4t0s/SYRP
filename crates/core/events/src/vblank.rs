// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`VBlankDriver`]: how a display peer takes over a peer's VBlank callbacks from the loop's
//! 60 Hz fallback timer.

use std::rc::{Rc, Weak};
use std::time::Duration;

use syrp_loop_api::{MessageThread, PeerId};

use crate::EventLoop;
use crate::event_loop::Inner;

/// Drives one peer's [`on_vblank`](syrp_loop_api::MessageLoop::on_vblank) callbacks from real
/// display refreshes. While any driver for a peer exists, the loop's fallback timer for that
/// peer is stopped; when the last one drops, the fallback resumes one refresh period later.
/// Made by [`EventLoop::vblank_driver`]; message thread only (`!Send`).
/// ```
/// use std::time::Duration;
/// use syrp_events::EventLoop;
/// use syrp_loop_api::{MessageLoop, PeerId};
/// let lp = EventLoop::new()?;
/// let peer = PeerId::from_raw(1);
/// let _g = lp.on_vblank(peer, Box::new(|_mt, at| assert_eq!(at, Duration::from_millis(7))));
/// let driver = lp.vblank_driver(peer);
/// assert_eq!(driver.fire(Duration::from_millis(7)), 1);
/// # Ok::<(), syrp_events::EventsError>(())
/// ```
#[doc(alias = "VBlankAttachment")]
pub struct VBlankDriver {
    inner: Weak<Inner>,
    peer: PeerId,
}

impl VBlankDriver {
    /// The peer this driver drives.
    /// ```
    /// let lp = syrp_events::EventLoop::new()?;
    /// let peer = syrp_loop_api::PeerId::from_raw(3);
    /// assert_eq!(lp.vblank_driver(peer).peer(), peer);
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    #[must_use]
    pub fn peer(&self) -> PeerId {
        self.peer
    }

    /// Reports one refresh at `at` (on the loop's clock, [`EventLoop::now`]): calls each of
    /// the peer's VBlank callbacks once, in registration order. Returns how many ran; 0 once
    /// the loop is gone. A callback may drop its own guard or make new callbacks; new ones
    /// run from the next refresh.
    ///
    /// # Panics
    /// If called off the loop's thread (the `!Send` driver makes that impossible in safe
    /// code).
    /// ```
    /// use syrp_loop_api::{MessageLoop, PeerId};
    /// let lp = syrp_events::EventLoop::new()?;
    /// let driver = lp.vblank_driver(PeerId::from_raw(1));
    /// assert_eq!(driver.fire(lp.now()), 0); // no callbacks yet
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    pub fn fire(&self, at: Duration) -> usize {
        let Some(inner) = self.inner.upgrade() else {
            return 0;
        };
        assert!(inner.is_message_thread(), "not the loop's thread");
        // SAFETY: we are part of this loop's implementation, on its message thread (checked
        // above), and the token is dropped when the callbacks return.
        let mt = unsafe { MessageThread::new_unchecked() };
        inner.fire_vblank(self.peer, at, &mt)
    }
}

impl Drop for VBlankDriver {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner.undrive(self.peer);
        }
    }
}

impl EventLoop {
    /// Lets a display peer (Wave 3) drive `peer`'s VBlank callbacks with
    /// [`VBlankDriver::fire`]; the 60 Hz fallback timer for `peer` stops while the driver
    /// lives. Several drivers for one peer may exist; the fallback resumes after the last.
    /// ```
    /// use std::{cell::Cell, rc::Rc, time::Duration};
    /// use syrp_loop_api::{MessageLoop, PeerId};
    /// let lp = syrp_events::EventLoop::new()?;
    /// let (peer, n) = (PeerId::from_raw(1), Rc::new(Cell::new(0)));
    /// let c = n.clone();
    /// let _g = lp.on_vblank(peer, Box::new(move |_mt, _at| c.set(c.get() + 1)));
    /// let driver = lp.vblank_driver(peer);
    /// lp.run_for(Duration::from_millis(50)); // the fallback would have fired about 3 times
    /// assert_eq!(n.get(), 0);
    /// driver.fire(lp.now());
    /// assert_eq!(n.get(), 1);
    /// # Ok::<(), syrp_events::EventsError>(())
    /// ```
    #[must_use]
    pub fn vblank_driver(&self, peer: PeerId) -> VBlankDriver {
        let inner: &Rc<Inner> = self.inner();
        inner.drive(peer);
        VBlankDriver {
            inner: Rc::downgrade(inner),
            peer,
        }
    }
}
