//! Quasi-static rectangular Track formulas. All lengths in any one calculation use the
//! same unit. The result is in ohms.

use std::f64::consts::{E, PI};

/// Hammerstad–Jensen surface microstrip with the finite-thickness effective width.
pub(super) fn surface(width: f64, thickness: f64, height: f64, dk: f64) -> f64 {
    let u = width / height;
    let tau = thickness / height;
    let delta_air = if tau == 0.0 {
        0.0
    } else {
        tau / PI * (4.0 * E / tau * (6.517 * u).sqrt().tanh().powi(2)).ln_1p()
    };
    let x = u + delta_air * 0.5 * (1.0 + 1.0 / (dk - 1.0).sqrt().cosh());
    let fringe = 6.0 + (2.0 * PI - 6.0) * (-(30.666 / x).powf(0.7528)).exp();
    let air_impedance = 376.730313668 / (2.0 * PI) * (fringe / x + 1.0_f64.hypot(2.0 / x)).ln();
    let fourth = x.powi(4);
    let a = 1.0
        + ((fourth + (x / 52.0).powi(2)) / (fourth + 0.432)).ln() / 49.0
        + (x / 18.1).powi(3).ln_1p() / 18.7;
    let beta = 0.564 * ((dk - 0.9) / (dk + 3.0)).powf(0.053);
    let effective_dk = (dk + 1.0) / 2.0 + (dk - 1.0) / 2.0 * (1.0 + 10.0 / x).powf(-a * beta);
    air_impedance / effective_dk.sqrt()
}

/// Wheeler 1978 stripline with the same clear dielectric gap on both sides.
pub(super) fn centred(width: f64, thickness: f64, gap: f64, dk: f64) -> f64 {
    let spacing = 2.0 * gap + thickness;
    let t = thickness / spacing;
    let added_width = if thickness == 0.0 {
        0.0
    } else {
        let m = 6.0 * (1.0 - t) / (3.0 - t);
        let q = (t / (2.0 - t)).powi(2) + (0.0796 * t / (width / spacing + 1.1 * t)).powf(m);
        thickness / PI * (1.0 - 0.5 * q.ln())
    };
    let x = 8.0 * (spacing - thickness) / (PI * (width + added_width));
    30.0 / dk.sqrt() * (0.5 * x * (x + (x * x + 6.27).sqrt())).ln_1p()
}

/// Offset stripline fit using both facing-surface gaps and their weighted permittivity.
pub(super) fn offset(width: f64, thickness: f64, top: f64, bottom: f64, dk: f64) -> f64 {
    let near = top.min(bottom);
    let far = top.max(bottom);
    80.0 / dk.sqrt()
        * (1.9 * (2.0 * near + thickness) / (0.8 * width + thickness)).ln()
        * (1.0 - near / (4.0 * far))
}

/// Brooks embedded microstrip with finite dielectric cover to the board surface.
pub(super) fn embedded(width: f64, thickness: f64, height: f64, cover: f64, dk: f64) -> f64 {
    let total = height + thickness + cover;
    let effective_dk = dk * -(-1.55 * total / height).exp_m1();
    60.0 / effective_dk.sqrt() * (5.98 * height / (0.8 * width + thickness)).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values in research/impedance-formulas, "Worked values for review", were
    // rounded to two decimal places. Allow 0.01 Ω for the comparison to those figures.
    fn reference(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 0.01,
            "{actual} Ω, expected {expected} Ω"
        );
    }

    #[test]
    fn surface_tracks_on_7628_and_3313_match_bare_copper_research_cases() {
        for (height, dk, width, expected) in [
            (0.2, 4.4, 0.2, 66.28),
            (0.2, 4.4, 0.3, 54.53),
            (0.2, 4.4, 0.35, 50.20),
            (0.1, 4.1, 0.1, 65.35),
            (0.1, 4.1, 0.15, 54.28),
            (0.1, 4.1, 0.2, 46.63),
            (0.794, 4.2, 1.5, 50.63),
        ] {
            reference(surface(width, 0.035, height, dk), expected);
        }
        reference(surface(0.35, 0.0, 0.2, 4.4), 52.72);
    }

    #[test]
    fn offset_track_matches_both_material_sensitivity_cases_in_either_direction() {
        for (dk, expected) in [(4.4, 48.87), (4.6, 47.79)] {
            reference(offset(0.25, 0.0152, 0.2104, 1.065, dk), expected);
            reference(offset(0.25, 0.0152, 1.065, 0.2104, dk), expected);
        }
    }

    #[test]
    fn centred_stripline_matches_cohns_ideal_at_equal_elliptic_moduli() {
        // With t = 0 and tanh(π w / 2b) = 1/√2, Cohn's complementary elliptic
        // integrals cancel. Wheeler's zero-thickness fit agreed within 0.3% in review.
        let width = 2.0 * 1.0_f64.asinh() / std::f64::consts::PI;
        let expected = 376.730313668 / (4.0 * 4.4_f64.sqrt());
        assert!((centred(width, 0.0, 0.5, 4.4) / expected - 1.0).abs() < 0.003);
        assert!(centred(width, 0.035, 0.4825, 4.4) < expected);
    }

    #[test]
    fn embedded_track_accounts_for_cover_above_copper() {
        reference(embedded(0.2, 0.035, 0.2, 0.3, 4.4), 52.30);
        assert!(embedded(0.2, 0.035, 0.2, 0.05, 4.4) > embedded(0.2, 0.035, 0.2, 0.3, 4.4));
    }
}
