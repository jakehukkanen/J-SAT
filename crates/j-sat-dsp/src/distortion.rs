//! Nonlinear waveshapers and output clipper for J-SAT:
//! - kHs-style Saturate (+10 dB drive, symmetric tanh, compensated)
//! - CrushClone / CamelCrusher Mech -> Tube cascade
//! - 1x Output Clipper (3 dB soft knee or 0 dBFS hard clip)

use crate::biquad::{Biquad, BiquadCoeffs};

/// +10 dB linear drive (`10^(10/20)`).
const KHS_SATURATE_DRIVE: f32 = 3.16227766;

/// `-3.0 dBFS` linear threshold (`10^(-3/20)`).
const SOFT_CLIP_THRESH: f32 = 0.70794578;
/// Input level where the C1-smooth quadratic knee reaches `1.0` (`2.0 - T`).
const SOFT_CLIP_MAX: f32 = 2.0 - SOFT_CLIP_THRESH;
/// `2 * (M - T) = 4 * (1 - T)` for `f(x) = T + over - over^2 / (2 * (M - T))`.
const SOFT_CLIP_KNEE_WIDTH_2: f32 = 4.0 * (1.0 - SOFT_CLIP_THRESH);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClipMode {
    #[default]
    Soft,
    Hard,
}

/// kHs Distortion "Saturate" clone at fixed +10 dB drive (hot into OTT 2).
#[inline]
pub fn khs_saturate(x: f32) -> f32 {
    (x * KHS_SATURATE_DRIVE).tanh()
}

/// Exact Mech waveshaper from `CrushClone/Source/DSP/Distortion.cpp`.
#[inline]
pub fn mech_clip(x: f32, drive: f32) -> f32 {
    let gain = 1.0 + drive * 29.0;
    let s = x * gain;
    if s > 1.0 {
        1.0
    } else if s < -1.0 {
        -1.0
    } else {
        let k = 0.15 * drive;
        s - k * (s * s * s)
    }
}

/// Exact Tube waveshaper from `CrushClone/Source/DSP/Distortion.cpp`.
#[inline]
pub fn tube_saturate(x: f32, drive: f32) -> f32 {
    let gain = 1.0 + drive * 19.0;
    let bias = drive * 0.08;
    let s = (x * gain + bias).tanh();
    s - bias.tanh()
}

/// Per-channel Mech pre-emphasis EQ + Mech -> Tube cascade (runs at 4x oversampled rate).
#[derive(Debug, Clone, Copy)]
pub struct MechTubeMono {
    mech_low_shelf: Biquad,
    mech_high_shelf: Biquad,
}

impl MechTubeMono {
    pub fn new(oversampled_rate: f64) -> Self {
        let mut s = Self {
            mech_low_shelf: Biquad::default(),
            mech_high_shelf: Biquad::default(),
        };
        s.set_oversampled_rate(oversampled_rate);
        s
    }

    pub fn set_oversampled_rate(&mut self, oversampled_rate: f64) {
        self.mech_low_shelf
            .set_coeffs(BiquadCoeffs::low_shelf_linear(
                oversampled_rate,
                200.0,
                0.7,
                1.8,
            ));
        self.mech_high_shelf
            .set_coeffs(BiquadCoeffs::high_shelf_linear(
                oversampled_rate,
                4000.0,
                0.7,
                2.0,
            ));
    }

    #[inline]
    pub fn reset(&mut self) {
        self.mech_low_shelf.reset();
        self.mech_high_shelf.reset();
    }

    /// Processes one sample at the 4x oversampled rate through post-OTT2 trim -> Mech -> Tube -> makeup.
    #[inline]
    pub fn process_oversampled(&mut self, x: f32, drive: f32) -> f32 {
        let trim_db = -40.0 + drive * 34.0;
        let trim_lin = 10.0_f32.powf(trim_db / 20.0);
        let g_total = trim_lin * (1.0 + drive * 29.0) * (1.0 + drive * 19.0);
        let makeup = 0.5 / (0.5 * g_total).tanh().max(1e-4);

        let m = self.mech_low_shelf.process(x * trim_lin);
        let m = self.mech_high_shelf.process(m);
        let m = mech_clip(m, drive);
        let t = tube_saturate(m, drive);
        t * makeup
    }
}

/// Final 1x output clipper (Soft 3 dB knee or Hard 0 dBFS clamp).
#[inline]
pub fn output_clip(x: f32, mode: ClipMode) -> f32 {
    match mode {
        ClipMode::Hard => x.clamp(-1.0, 1.0),
        ClipMode::Soft => {
            let abs_x = x.abs();
            if abs_x <= SOFT_CLIP_THRESH {
                x
            } else if abs_x >= SOFT_CLIP_MAX {
                x.signum()
            } else {
                let over = abs_x - SOFT_CLIP_THRESH;
                let y = SOFT_CLIP_THRESH + over - (over * over) / SOFT_CLIP_KNEE_WIDTH_2;
                x.signum() * y.min(1.0)
            }
        }
    }
}
