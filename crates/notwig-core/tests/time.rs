use notwig_core::{Db, Pan, Rational, SampleRate, TempoMap};

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

#[test]
fn db_gain_f32_and_f64_agree() {
    for db in [
        -96.0_f32, -60.0, -24.0, -6.0, -1.0, 0.0, 1.0, 6.0, 24.0, 48.0,
    ] {
        let f32_gain = f64::from(Db(db).to_gain().0);
        let f64_gain = Db(db).to_gain_f64();
        assert!(
            (f32_gain - f64_gain).abs() <= f64_gain.abs() * 1e-6,
            "db {db}: f32 gain {f32_gain} vs f64 gain {f64_gain}"
        );
    }
}
