use notwig_core::{Pan, Rational, SampleRate, TempoMap};

#[test]
fn rational_normalizes() {
    assert_eq!(Rational::new(2, 4), Rational::new(1, 2));
    assert_eq!(Rational::new(-2, 4), Rational::new(-1, 2));
}

#[test]
fn tempo_map_roundtrip_is_near_exact() {
    let map = TempoMap {
        tempo_bpm: 120.0,
        sample_rate: SampleRate::DEFAULT,
    };
    assert_eq!(map.mus_to_samples(Rational::new(1, 1)), 24_000);
    assert_eq!(map.samples_to_mus(24_000), Rational::new(1, 1));
}

#[test]
fn constant_power_pan_is_minus_three_db_center() {
    let (l, r) = Pan(0.0).gains();
    let channel_db = 20.0 * l.log10();
    assert!((channel_db + 3.0103).abs() < 0.01);
    assert!((l - r).abs() < 1e-6);
}
