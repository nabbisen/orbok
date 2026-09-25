//! UTC timestamp helpers (external design §9.3: ISO-8601 UTC strings).
//!
//! **Fixed width (Task 116).** Every timestamp orbok writes is RFC 3339 UTC
//! with **exactly nine fractional digits** and `Z`, zero fraction included:
//! `2026-09-24T15:26:29.123400000Z`, always 30 characters. `time`'s own RFC 3339
//! writes the fraction with trailing zeros trimmed, and not at all when it is
//! zero, so the strings varied in width and, compared as text (which is how
//! SQLite compares them), a later moment could sort earlier
//! (`…29.123456Z` < `…29.1234Z`). With one width, text order is time order for
//! every value orbok writes.

use std::cell::RefCell;
use std::time::SystemTime;
use time::OffsetDateTime;

type Clock = Box<dyn FnMut() -> SystemTime>;

thread_local! {
    /// The clock [`now_iso8601`] reads, when a test has replaced it.
    static CLOCK: RefCell<Option<Clock>> = const { RefCell::new(None) };
}

/// **Tests only.** Run `f` with [`now_iso8601`] reading `clock` on this thread,
/// so a test can say exactly which instants a scan sees (Task 116 §2.2).
/// Nothing in the product calls it.
#[doc(hidden)]
pub fn with_clock<R>(clock: impl FnMut() -> SystemTime + 'static, f: impl FnOnce() -> R) -> R {
    let previous = CLOCK.with(|c| c.borrow_mut().replace(Box::new(clock)));
    struct Restore(Option<Clock>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CLOCK.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(previous);
    f()
}

/// The fixed-width text of `t`. `None` for a year outside `0000..=9999`,
/// which would not be four digits wide.
fn fixed_width(t: OffsetDateTime) -> Option<String> {
    let t = t.to_offset(time::UtcOffset::UTC);
    (0..=9999).contains(&t.year()).then(|| {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
            t.year(),
            u8::from(t.month()),
            t.day(),
            t.hour(),
            t.minute(),
            t.second(),
            t.nanosecond()
        )
    })
}

const EPOCH: &str = "1970-01-01T00:00:00.000000000Z";

/// Current UTC time, fixed width, e.g. `2026-06-06T12:34:56.789000000Z`.
pub fn now_iso8601() -> String {
    let overridden = CLOCK.with(|c| c.borrow_mut().as_mut().map(|clock| clock()));
    match overridden {
        Some(t) => system_time_iso8601(t),
        None => fixed_width(OffsetDateTime::now_utc()).unwrap_or_else(|| EPOCH.to_string()),
    }
}

/// Convert a [`std::time::SystemTime`] (e.g. file mtime) to the same fixed
/// width text.
pub fn system_time_iso8601(t: SystemTime) -> String {
    fixed_width(OffsetDateTime::from(t)).unwrap_or_else(|| EPOCH.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    /// Task 116 §2.1: a zero-nanosecond instant and a trailing-zero instant
    /// come out the same width, so text order can be time order.
    #[test]
    fn every_instant_is_written_thirty_characters_wide() {
        for nanos in [0u32, 1, 100, 123_400_000, 123_456_000, 999_999_999] {
            let t = UNIX_EPOCH + Duration::new(1_790_000_000, nanos);
            let text = system_time_iso8601(t);
            assert_eq!(text.len(), 30, "{text}");
            assert!(text.ends_with('Z'), "{text}");
        }
        assert_eq!(now_iso8601().len(), 30);
        assert_eq!(
            system_time_iso8601(UNIX_EPOCH + Duration::new(1_790_000_000, 123_400_000)),
            "2026-09-21T14:13:20.123400000Z"
        );
    }

    /// For a spread of instants, including the pair from the task's §0, text
    /// order equals time order.
    #[test]
    fn text_order_is_time_order() {
        let base = 1_790_000_000;
        let mut instants: Vec<std::time::SystemTime> = Vec::new();
        for secs in [base, base + 1, base + 60, base + 86_400] {
            for nanos in [
                0,
                1,
                999,
                123_400_000,
                123_456_000,
                123_456_789,
                500_000_000,
            ] {
                instants.push(UNIX_EPOCH + Duration::new(secs, nanos));
            }
        }
        for a in &instants {
            for b in &instants {
                assert_eq!(
                    system_time_iso8601(*a).cmp(&system_time_iso8601(*b)),
                    a.cmp(b),
                    "{a:?} vs {b:?}"
                );
            }
        }
        // The pair in the task: `.123456` is later than `.1234`, and must sort so.
        let early = UNIX_EPOCH + Duration::new(base, 123_400_000);
        let late = UNIX_EPOCH + Duration::new(base, 123_456_000);
        assert!(system_time_iso8601(early) < system_time_iso8601(late));
    }
}
