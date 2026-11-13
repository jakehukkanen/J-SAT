//! Zero-allocation multi-stage "Phat Rack" DSP chain for J-SAT.

pub mod biquad;
pub mod distortion;
pub mod ott;
pub mod oversample;

use biquad::{Biquad, BiquadCoeffs, DcBlocker};
use distortion::{MechTubeMono, khs_saturate, output_clip};
use ott::OttMono;
use oversample::Oversampler4x;

pub use distortion::ClipMode;

/// 8th-order Butterworth high-pass Q values (4 cascaded 2nd-order sections = 48 dB/oct).
const SUB_CUT_QS: [f64; 4] = [0.5098, 0.6013, 0.8999, 2.5629];
const SUB_CUT_FREQ_HZ: f64 = 135.0;

const TREBLE_FREQ_HZ: f64 = 6000.0;
const TREBLE_Q: f64 = 0.707;

const PEAK1_FREQ_HZ: f64 = 2000.0;
const PEAK1_GAIN_DB: f64 = -10.0;
const PEAK1_Q: f64 = 2.42;

const PEAK2_FREQ_HZ: f64 = 3200.0;
const PEAK2_GAIN_DB: f64 = -10.0;
const PEAK2_Q: f64 = 5.87;

const DC_BLOCKER_HZ: f64 = 7.5;

/// Smoothed per-block DSP parameters for [`JSatProcessor`].
#[derive(Debug, Clone, Copy)]
pub struct JSatParams {
    pub pre_gain_db: f32,
    pub drive: f32,
    pub treble_db: f32,
    pub notch_mix: f32,
    pub sub_cut: bool,
    pub clip_mode: ClipMode,
}

impl Default for JSatParams {
    fn default() -> Self {
        Self {
            pre_gain_db: 0.0,
            drive: 0.40,
            treble_db: 0.0,
            notch_mix: 0.0,
            sub_cut: false,
            clip_mode: ClipMode::Soft,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct JSatChannel {
    ott1: OttMono,
    sat_os: Oversampler4x,
    sat_dc: DcBlocker,
    ott2: OttMono,
    treble_shelf: Biquad,
    mech_tube_os: Oversampler4x,
    mech_tube: MechTubeMono,
    peak1: Biquad,
    peak2: Biquad,
    sub_cut_hpf: [Biquad; 4],
}

impl JSatChannel {
    fn new(sample_rate: f64) -> Self {
        let mut ch = Self {
            ott1: OttMono::new(sample_rate, 0.33, 10.0),
            sat_os: Oversampler4x::new(),
            sat_dc: DcBlocker::new(sample_rate, DC_BLOCKER_HZ),
            ott2: OttMono::new(sample_rate, 1.0, 10.0),
            treble_shelf: Biquad::default(),
            mech_tube_os: Oversampler4x::new(),
            mech_tube: MechTubeMono::new(sample_rate * 4.0),
            peak1: Biquad::default(),
            peak2: Biquad::default(),
            sub_cut_hpf: [Biquad::default(); 4],
        };
        ch.set_sample_rate(sample_rate, 0.0);
        ch
    }

    fn set_sample_rate(&mut self, sample_rate: f64, treble_db: f64) {
        self.ott1.set_sample_rate(sample_rate);
        self.sat_dc.set_sample_rate(sample_rate, DC_BLOCKER_HZ);
        self.ott2.set_sample_rate(sample_rate);
        self.treble_shelf.set_coeffs(BiquadCoeffs::high_shelf_db(
            sample_rate,
            TREBLE_FREQ_HZ,
            TREBLE_Q,
            treble_db,
        ));
        self.mech_tube.set_oversampled_rate(sample_rate * 4.0);
        self.peak1.set_coeffs(BiquadCoeffs::peaking_db(
            sample_rate,
            PEAK1_FREQ_HZ,
            PEAK1_Q,
            PEAK1_GAIN_DB,
        ));
        self.peak2.set_coeffs(BiquadCoeffs::peaking_db(
            sample_rate,
            PEAK2_FREQ_HZ,
            PEAK2_Q,
            PEAK2_GAIN_DB,
        ));
        for (biquad, &q) in self.sub_cut_hpf.iter_mut().zip(SUB_CUT_QS.iter()) {
            biquad.set_coeffs(BiquadCoeffs::high_pass(sample_rate, SUB_CUT_FREQ_HZ, q));
        }
    }

    fn reset(&mut self) {
        self.ott1.reset();
        self.sat_os.reset();
        self.sat_dc.reset();
        self.ott2.reset();
        self.treble_shelf.reset();
        self.mech_tube_os.reset();
        self.mech_tube.reset();
        self.peak1.reset();
        self.peak2.reset();
        for biquad in &mut self.sub_cut_hpf {
            biquad.reset();
        }
    }

    #[inline]
    fn update_treble(&mut self, sample_rate: f64, treble_db: f64) {
        self.treble_shelf.set_coeffs(BiquadCoeffs::high_shelf_db(
            sample_rate,
            TREBLE_FREQ_HZ,
            TREBLE_Q,
            treble_db,
        ));
    }

    #[inline]
    fn process_sample(
        &mut self,
        input: f32,
        pre_gain_lin: f32,
        drive: f32,
        notch_mix: f32,
        sub_cut: bool,
        clip_mode: ClipMode,
    ) -> f32 {
        // 1. Pre-Gain
        let mut x = input * pre_gain_lin;

        // 2. OTT Stage 1 (Subtle Glue: Depth = 33%, Time = 1000%)
        x = self.ott1.process(x);

        // 3. Saturate Stage (kHs Distortion "Saturate" Clone, 4x oversampled + 1-pole DC blocker)
        x = self.sat_os.process_sample(x, khs_saturate);
        x = self.sat_dc.process(x);

        // 4. OTT Stage 2 (Full OTT: Depth = 100%, Time = 1000%)
        x = self.ott2.process(x);

        // 5. Treble High Shelf (6000 Hz, Q = 0.707)
        x = self.treble_shelf.process(x);

        // 6. Dual CamelCrusher Waveshapers (post-OTT2 trim -> Mech -> Tube, 4x oversampled)
        let mt = &mut self.mech_tube;
        x = self
            .mech_tube_os
            .process_sample(x, |s| mt.process_oversampled(s, drive));

        // 7. Harshness Taming Peak Filters (Parallel Mix)
        if notch_mix > 1e-5 {
            let wet = self.peak2.process(self.peak1.process(x));
            x = x * (1.0 - notch_mix) + wet * notch_mix;
        }

        // 8. Sub Cut High-Pass Filter (135 Hz, 48 dB/oct Butterworth)
        if sub_cut {
            for biquad in &mut self.sub_cut_hpf {
                x = biquad.process(x);
            }
        }

        // 9. Output Clipper (1x native sample rate, NO oversampling)
        output_clip(x, clip_mode)
    }
}

/// Stereo zero-allocation J-SAT processor.
pub struct JSatProcessor {
    sample_rate: f64,
    last_treble_db: f32,
    channels: [JSatChannel; 2],
}

impl JSatProcessor {
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            sample_rate: sr,
            last_treble_db: 0.0,
            channels: [JSatChannel::new(sr), JSatChannel::new(sr)],
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(8000.0);
        let treble_db = self.last_treble_db as f64;
        for ch in &mut self.channels {
            ch.set_sample_rate(self.sample_rate, treble_db);
        }
    }

    pub fn reset(&mut self) {
        for ch in &mut self.channels {
            ch.reset();
        }
    }

    /// Processes a stereo block in-place with zero heap allocations.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32], params: &JSatParams) {
        if (params.treble_db - self.last_treble_db).abs() > 1e-4 {
            self.last_treble_db = params.treble_db;
            let treble_db = params.treble_db as f64;
            for ch in &mut self.channels {
                ch.update_treble(self.sample_rate, treble_db);
            }
        }

        let pre_gain_lin = 10.0_f32.powf(params.pre_gain_db / 20.0);
        let drive = params.drive.clamp(0.0, 1.0);
        let notch_mix = params.notch_mix.clamp(0.0, 1.0);

        let len = left.len().min(right.len());
        for i in 0..len {
            left[i] = self.channels[0].process_sample(
                left[i],
                pre_gain_lin,
                drive,
                notch_mix,
                params.sub_cut,
                params.clip_mode,
            );
            right[i] = self.channels[1].process_sample(
                right[i],
                pre_gain_lin,
                drive,
                notch_mix,
                params.sub_cut,
                params.clip_mode,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn silence_stays_finite_and_zero() {
        let mut proc = JSatProcessor::new(48000.0);
        let mut l = [0.0f32; 512];
        let mut r = [0.0f32; 512];
        proc.process_block(&mut l, &mut r, &JSatParams::default());
        for (&sl, &sr) in l.iter().zip(r.iter()) {
            assert!(sl.is_finite() && sl == 0.0);
            assert!(sr.is_finite() && sr == 0.0);
        }
    }

    #[test]
    fn output_never_exceeds_zero_dbfs() {
        for mode in [ClipMode::Soft, ClipMode::Hard] {
            let mut proc = JSatProcessor::new(44100.0);
            let mut l = [0.0f32; 2048];
            let mut r = [0.0f32; 2048];
            for i in 0..l.len() {
                let s = (2.0 * PI * 110.0 * i as f32 / 44100.0).sin() * 2.0;
                l[i] = s;
                r[i] = -s;
            }
            let params = JSatParams {
                pre_gain_db: 24.0,
                drive: 1.0,
                treble_db: 6.0,
                notch_mix: 1.0,
                sub_cut: true,
                clip_mode: mode,
            };
            proc.process_block(&mut l, &mut r, &params);
            for (&sl, &sr) in l.iter().zip(r.iter()) {
                assert!(sl.is_finite() && sl.abs() <= 1.0, "left={sl}");
                assert!(sr.is_finite() && sr.abs() <= 1.0, "right={sr}");
            }
        }
    }

    #[test]
    fn sub_cut_attenuates_50hz_strongly() {
        let sr = 48000.0;
        let mut hpf: [Biquad; 4] = std::array::from_fn(|i| {
            Biquad::new(BiquadCoeffs::high_pass(sr, SUB_CUT_FREQ_HZ, SUB_CUT_QS[i]))
        });
        let n = 8192;
        let mut sum_sq = 0.0f32;
        for i in 0..n {
            let mut s = (2.0 * PI * 50.0 * i as f32 / sr as f32).sin();
            for b in &mut hpf {
                s = b.process(s);
            }
            if i >= n / 2 {
                sum_sq += s * s;
            }
        }
        let tail_rms = (sum_sq / (n / 2) as f32).sqrt();
        // At 50 Hz with 135 Hz 48 dB/oct Butterworth: ~ -34.5 dB -> RMS ~0.013 (vs 0.707 input)
        assert!(
            tail_rms < 0.02,
            "50 Hz should be attenuated >30 dB by 135 Hz 48 dB/oct HPF, got {tail_rms}"
        );
    }

    #[test]
    fn soft_clipper_knee_is_monotonic_and_bounded() {
        let mut prev = -1.0f32;
        for i in -200..=200 {
            let x = i as f32 * 0.01;
            let y = output_clip(x, ClipMode::Soft);
            assert!(y.abs() <= 1.0, "out of bounds at x={x}: y={y}");
            assert!(y >= prev - 1e-6, "non-monotonic at x={x}: {y} < {prev}");
            prev = y;
        }
        // Below -3 dBFS (~0.7079), soft clipper is linear
        assert!((output_clip(0.5, ClipMode::Soft) - 0.5).abs() < 1e-6);
        assert!((output_clip(-0.5, ClipMode::Soft) - (-0.5)).abs() < 1e-6);
    }
}
