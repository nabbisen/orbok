//! RFC-057 §6.2 item 4: battery detection tested at the injection seam,
//! both states, no real battery required. `SystemBatterySource` itself
//! gets only a does-not-panic smoke test below -- its actual reading is
//! real hardware/CI-runner state, uncontrollable and not asserted on
//! (RFC-057 §6.1).

use super::*;
use futures::StreamExt;
use std::collections::VecDeque;

/// A scripted [`BatterySource`]: returns each queued reading in order,
/// then repeats the last one forever once exhausted, so a test that polls
/// a few extra times past the script's end doesn't panic.
struct ScriptedBatterySource {
    readings: VecDeque<Option<bool>>,
}

impl ScriptedBatterySource {
    fn new(readings: Vec<Option<bool>>) -> Self {
        assert!(!readings.is_empty(), "a script needs at least one reading");
        Self {
            readings: readings.into(),
        }
    }
}

impl BatterySource for ScriptedBatterySource {
    fn is_on_battery(&mut self) -> Option<bool> {
        if self.readings.len() > 1 {
            self.readings.pop_front().unwrap()
        } else {
            *self.readings.front().unwrap()
        }
    }
}

const POLL: Duration = Duration::from_millis(5);

async fn next_observation(
    rx: &mut futures::channel::mpsc::Receiver<ResourceObservation>,
) -> ResourceObservation {
    tokio::time::timeout(Duration::from_secs(2), rx.next())
        .await
        .expect("an observation must arrive within the timeout")
        .expect("the channel must not close before an observation is sent")
}

// RFC-057 §6.2 item 4 (both halves): a transition to "on battery" and
// back must each produce exactly one `OnBattery` observation, proving the
// detector-side half of what "both states, no real battery required"
// requires.
#[tokio::test]
async fn sends_an_observation_on_each_transition() {
    let source = ScriptedBatterySource::new(vec![
        Some(false),
        Some(false),
        Some(true),
        Some(true),
        Some(true),
        Some(false),
    ]);
    let (tx, mut rx) = futures::channel::mpsc::channel(16);
    let handle = tokio::spawn(watch_battery(source, POLL, tx));

    assert_eq!(
        next_observation(&mut rx).await,
        ResourceObservation::OnBattery(true),
        "the false -> true transition must be reported"
    );
    assert_eq!(
        next_observation(&mut rx).await,
        ResourceObservation::OnBattery(false),
        "the true -> false transition must be reported"
    );

    handle.abort();
}

// The poller's initial assumption is "not on battery" -- a source that
// never disagrees with that must never send anything, not even once at
// startup. Proves the change-gate isn't merely "skip immediate repeats"
// but genuinely compares against the last known state.
#[tokio::test]
async fn no_observation_when_state_never_changes() {
    let source = ScriptedBatterySource::new(vec![Some(false)]);
    let (tx, mut rx) = futures::channel::mpsc::channel(16);
    let handle = tokio::spawn(watch_battery(source, POLL, tx));

    tokio::time::sleep(POLL * 20).await;
    assert!(
        rx.try_recv().is_err(),
        "no observation must be sent when the state never changes"
    );

    handle.abort();
}

// RFC-057 §4.3d's fail-open contract: a source that can never determine
// the state (e.g. a desktop with no battery -- this development machine
// is exactly this case, confirmed via `upower`) must behave identically
// to "never on battery," not stall or panic.
#[tokio::test]
async fn undetermined_readings_never_produce_an_observation() {
    let source = ScriptedBatterySource::new(vec![None, None, None]);
    let (tx, mut rx) = futures::channel::mpsc::channel(16);
    let handle = tokio::spawn(watch_battery(source, POLL, tx));

    tokio::time::sleep(POLL * 20).await;
    assert!(
        rx.try_recv().is_err(),
        "an undetermined reading must never itself be sent, and must not \
         be treated as a change from the false starting assumption"
    );

    handle.abort();
}

// Repeated identical readings across many polls must not spam the
// channel -- only genuine transitions are observations, matching the
// same "no duplicate events" discipline RFC-036 §12.1 already requires
// of `notify_user_active` (`repeated_user_active_does_not_spam_events`,
// `rfc036_scheduler.rs`).
#[tokio::test]
async fn repeated_identical_readings_do_not_spam_observations() {
    let source = ScriptedBatterySource::new(vec![Some(true); 20]);
    let (tx, mut rx) = futures::channel::mpsc::channel(64);
    let handle = tokio::spawn(watch_battery(source, POLL, tx));

    assert_eq!(
        next_observation(&mut rx).await,
        ResourceObservation::OnBattery(true),
        "the single false -> true transition must be reported"
    );
    tokio::time::sleep(POLL * 15).await;
    assert!(
        rx.try_recv().is_err(),
        "no further observation once already on battery and the reading stays true"
    );

    handle.abort();
}

// RFC-057 §4.3d: production detection must degrade gracefully -- an
// `Option`, never a panic -- on whatever machine runs it. What it
// actually returns is real hardware/CI-runner state and deliberately not
// asserted on (RFC-057 §6.1).
#[test]
fn system_battery_source_does_not_panic() {
    let mut source = SystemBatterySource::new();
    let _ = source.is_on_battery();
}
