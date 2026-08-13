//! RFC-057 §4.3d: battery/power-source detection, behind an injection
//! seam -- battery state is not controllable in CI (RFC-057 §6.1), so
//! production and tests inject different [`BatterySource`]
//! implementations rather than [`watch_battery`] querying an OS API
//! directly. The same shape `RuntimePathProbe`/`AllowRuntimePathProbe`
//! (RFC-049, `crate::runtime_context`) already establish in this
//! codebase for the same reason.

use super::ResourceObservation;
use futures::SinkExt as _;
use futures::channel::mpsc::Sender;
use std::time::Duration;

/// How often the poller re-checks power-source state. Battery state
/// changes on the order of minutes (plugging/unplugging), not seconds --
/// this only needs to notice eventually, and a coarse interval keeps the
/// OS query off the hot path.
pub(crate) const BATTERY_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// A live power-source detector. `is_on_battery` returns `None` when the
/// state cannot be determined -- no battery present (a desktop machine),
/// or the platform query failed -- treated as "not on battery" by
/// [`watch_battery`], the same fail-open choice `pause_on_battery`'s
/// original doc comment already reasoned about for "no signal source."
pub(crate) trait BatterySource: Send + 'static {
    fn is_on_battery(&mut self) -> Option<bool>;
}

/// The production detector, backed by `starship-battery` (RFC-057 §4.3d
/// dependency decision -- maintenance, platform coverage and licence are
/// recorded in the review request, checked against the lockfile). Holds
/// the `Manager` across calls: it is a system handle, not per-query
/// state, and constructing one afresh every poll would repeat OS-level
/// setup `starship-battery` itself only expects to pay once.
pub(crate) struct SystemBatterySource {
    manager: Option<starship_battery::Manager>,
}

impl SystemBatterySource {
    pub(crate) fn new() -> Self {
        // A `Manager` that fails to construct (an unsupported platform,
        // or the OS API is unavailable) degrades to `is_on_battery`
        // always returning `None` -- fail-open, not a startup error,
        // since RFC-057 §4.3 already documents that battery detection
        // may legitimately be absent.
        Self {
            manager: starship_battery::Manager::new().ok(),
        }
    }
}

impl BatterySource for SystemBatterySource {
    fn is_on_battery(&mut self) -> Option<bool> {
        let manager = self.manager.as_ref()?;
        // The first battery only (RFC-057 §4.3d): almost every machine
        // this targets has zero or one. A machine with several would need
        // a disagreement policy between them, out of scope for a P0
        // detector -- RFC-036 §13.2 itself only ever names one
        // battery-backed state.
        let mut battery = manager.batteries().ok()?.next()?.ok()?;
        manager.refresh(&mut battery).ok()?;
        match battery.state() {
            starship_battery::State::Discharging | starship_battery::State::Empty => Some(true),
            starship_battery::State::Charging | starship_battery::State::Full => Some(false),
            starship_battery::State::Unknown => None,
        }
    }
}

/// Poll `source` every `interval` and send an `OnBattery` observation
/// whenever the detected state *changes* -- not on every poll, so an
/// idle machine doesn't add the channel traffic RFC-057 §8 warns against.
/// Runs until `tx` closes (process shutdown). A `None` reading is folded
/// to "not on battery" before the comparison, matching
/// `BatterySource::is_on_battery`'s documented fail-open contract; it is
/// sent only if that itself is a change from the last known state, the
/// same as any other reading.
pub(crate) async fn watch_battery(
    mut source: impl BatterySource,
    interval: Duration,
    mut tx: Sender<ResourceObservation>,
) {
    let mut last_known = false;
    loop {
        let on_battery = source.is_on_battery().unwrap_or(false);
        if on_battery != last_known {
            last_known = on_battery;
            if tx
                .send(ResourceObservation::OnBattery(on_battery))
                .await
                .is_err()
            {
                return; // Receiver dropped -- nothing left to notify.
            }
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests;
