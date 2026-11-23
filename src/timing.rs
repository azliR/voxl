//! Where a frame's latency goes, for the viewer's stats overlay: every half second each
//! viewer gets `{"t":"timing",...}` with the server's stage times averaged over that
//! half second, and the connection's round trip.
//!
//! - `wait`: from the screen changing to its capture starting (frame pacing, flow control)
//! - `grab`: copying the changed rows out of the capture
//! - `convert`: BGRX to I420
//! - `encode`: x264, all stripes of a frame
//! - `rtt`: the latest ping's round trip, `minRtt` the recent minimum, `queue` how far
//!   acks lag behind that minimum (data queued on the way)
//!
//! Stage times are in milliseconds, `null` when no frame went through that half second.

use crate::hub::{Client, Stats};
use std::sync::atomic::Ordering;

/// The hub's running totals at one moment.
#[derive(Default, Clone, Copy)]
pub struct Snapshot {
    captured: u64,
    encoded: u64,
    wait_us: u64,
    grab_us: u64,
    convert_us: u64,
    encode_us: u64,
}

impl Snapshot {
    pub fn take(stats: &Stats) -> Self {
        Snapshot {
            captured: stats.captured.load(Ordering::Relaxed),
            encoded: stats.encoded.load(Ordering::Relaxed),
            wait_us: stats.damage_wait_us.load(Ordering::Relaxed),
            grab_us: stats.grab_us.load(Ordering::Relaxed),
            convert_us: stats.convert_us.load(Ordering::Relaxed),
            encode_us: stats.encode_us.load(Ordering::Relaxed),
        }
    }
}

/// Average in milliseconds of `total` microseconds over `count` events.
fn average_ms(total: u64, count: u64) -> Option<f64> {
    (count > 0).then(|| total as f64 / count as f64 / 1000.0)
}

/// The timing message for the time between `before` and `after`.
pub fn message(before: &Snapshot, after: &Snapshot, client: &Client) -> String {
    let captured = after.captured.wrapping_sub(before.captured);
    let encoded = after.encoded.wrapping_sub(before.encoded);
    let per_capture = |a: u64, b: u64| average_ms(a.wrapping_sub(b), captured);
    serde_json::json!({
        "t": "timing",
        "wait": per_capture(after.wait_us, before.wait_us),
        "grab": per_capture(after.grab_us, before.grab_us),
        "convert": per_capture(after.convert_us, before.convert_us),
        "encode": average_ms(after.encode_us.wrapping_sub(before.encode_us), encoded),
        "rtt": client.rtt_us.load(Ordering::Relaxed) as f64 / 1000.0,
        "minRtt": client.min_rtt_ms.load(Ordering::Relaxed),
        "queue": client.delay.lock().queue_ms,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_only_what_happened_in_between() {
        let before = Snapshot {
            captured: 10,
            encoded: 10,
            wait_us: 10_000,
            grab_us: 5_000,
            convert_us: 2_000,
            encode_us: 40_000,
        };
        let after = Snapshot {
            captured: 14,
            encoded: 12,
            wait_us: 14_000,
            grab_us: 7_000,
            convert_us: 4_000,
            encode_us: 50_000,
        };
        assert_eq!(average_ms(after.wait_us - before.wait_us, 4), Some(1.0));
        assert_eq!(average_ms(after.encode_us - before.encode_us, 2), Some(5.0));
        assert_eq!(average_ms(123, 0), None);
    }
}
