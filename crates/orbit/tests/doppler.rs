//! Expected values are the classical Doppler relation f' = f (1 -/+ v/c), worked with awk
//! (c = 299792.458 km/s), independent of the crate. Range rate is positive when receding.

use orbit::{downlink_hz, uplink_hz};

fn assert_close(got: f64, want: f64) {
    assert!((got - want).abs() < 0.1, "got {got}, want {want}");
}

#[test]
fn receding_satellite_lowers_the_downlink_and_raises_the_needed_uplink() {
    assert_close(downlink_hz(435_000_000.0, 3.0), 434_995_646.988_6);
    assert_close(uplink_hz(145_900_000.0, 3.0), 145_901_460.010_0);
}

#[test]
fn approaching_satellite_raises_the_downlink_and_lowers_the_needed_uplink() {
    assert_close(downlink_hz(145_800_000.0, -6.55728), 145_803_189.044_3);
    assert_close(uplink_hz(435_300_000.0, -6.55728), 435_290_478.799_9);
}

#[test]
fn zero_range_rate_leaves_frequencies_unchanged() {
    assert_close(downlink_hz(145_800_000.0, 0.0), 145_800_000.0);
    assert_close(uplink_hz(145_800_000.0, 0.0), 145_800_000.0);
}
