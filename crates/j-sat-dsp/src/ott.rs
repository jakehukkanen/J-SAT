//! 3-Band OTT Multiband Compressor ported from `vitOTTx` (Vital DSP).
//!
//! Uses 4th-order Linkwitz-Riley crossovers (120 Hz & 2500 Hz) with phase alignment
//! and per-band upward + downward RMS envelope followers.

use std::f64::consts::{PI, SQRT_2};

const K_MAX_EXPAND_MULT: f64 = 32.0;
const K_MIN_SAMPLE_ENVELOPE: f64 = 5.0;

#[inline]
fn db_to_gain(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

#[derive(Debug, Clone, Copy, Default)]
struct LrBiquad {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl LrBiquad {
    #[inline]
    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    #[inline]
    fn process(&mut self, x: f64, b0: f64, b1: f64, b2: f64, out1: f64, out2: f64) -> f64 {
        let y = b0 * x + b1 * self.x1 + b2 * self.x2 + out1 * self.y1 + out2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// 4th-order Linkwitz-Riley crossover (two cascaded 2nd-order Butterworth LP/HP pairs).
#[derive(Debug, Clone, Copy)]
struct LinkwitzRiley4 {
    cutoff_hz: f64,
    low_in_0: f64,
    low_in_1: f64,
    low_in_2: f64,
    low_out_1: f64,
    low_out_2: f64,
    high_in_0: f64,
    high_in_1: f64,
    high_in_2: f64,
    low_a: LrBiquad,
    low_b: LrBiquad,
    high_a: LrBiquad,
    high_b: LrBiquad,
}

impl LinkwitzRiley4 {
    fn new(cutoff_hz: f64, sample_rate: f64) -> Self {
        let mut s = Self {
            cutoff_hz,
            low_in_0: 0.0,
            low_in_1: 0.0,
            low_in_2: 0.0,
            low_out_1: 0.0,
            low_out_2: 0.0,
            high_in_0: 0.0,
            high_in_1: 0.0,
            high_in_2: 0.0,
            low_a: LrBiquad::default(),
            low_b: LrBiquad::default(),
            high_a: LrBiquad::default(),
            high_b: LrBiquad::default(),
        };
        s.set_sample_rate(sample_rate);
        s
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        let freq = self.cutoff_hz.clamp(1.0, sample_rate * 0.45);
        let warp = 1.0 / (PI * freq / sample_rate).tan();
        let warp2 = warp * warp;
        let mult = 1.0 / (1.0 + SQRT_2 * warp + warp2);

        self.low_in_0 = mult;
        self.low_in_1 = 2.0 * mult;
        self.low_in_2 = mult;
        self.low_out_1 = -2.0 * (1.0 - warp2) * mult;
        self.low_out_2 = -(1.0 - SQRT_2 * warp + warp2) * mult;

        self.high_in_0 = warp2 * mult;
        self.high_in_1 = -2.0 * self.high_in_0;
        self.high_in_2 = self.high_in_0;
    }

    #[inline]
    fn reset(&mut self) {
        self.low_a.reset();
        self.low_b.reset();
        self.high_a.reset();
        self.high_b.reset();
    }

    #[inline]
    fn split(&mut self, x: f64) -> (f64, f64) {
        let lp1 = self.low_a.process(
            x,
            self.low_in_0,
            self.low_in_1,
            self.low_in_2,
            self.low_out_1,
            self.low_out_2,
        );
        let lp = self.low_b.process(
            lp1,
            self.low_in_0,
            self.low_in_1,
            self.low_in_2,
            self.low_out_1,
            self.low_out_2,
        );

        let hp1 = self.high_a.process(
            x,
            self.high_in_0,
            self.high_in_1,
            self.high_in_2,
            self.low_out_1,
            self.low_out_2,
        );
        let hp = self.high_b.process(
            hp1,
            self.high_in_0,
            self.high_in_1,
            self.high_in_2,
            self.low_out_1,
            self.low_out_2,
        );

        (lp, hp)
    }
}

#[derive(Debug, Clone, Copy)]
struct BandParams {
    base_attack_ms: f64,
    base_release_ms: f64,
    lower_thresh_db: f64,
    upper_thresh_db: f64,
    lower_ratio: f64,
    upper_ratio: f64,
    output_gain_db: f64,
}

#[derive(Debug, Clone, Copy)]
struct OttBandCompressor {
    params: BandParams,
    env_attack_samples: f64,
    env_release_samples: f64,
    attack_scale: f64,
    release_scale: f64,
    upper_thresh_sq: f64,
    lower_thresh_sq: f64,
    upper_ratio_pow: f64,
    lower_ratio_pow: f64,
    output_mult: f64,
    high_env_sq: f64,
    low_env_sq: f64,
}

impl OttBandCompressor {
    fn new(params: BandParams, sample_rate: f64, time_mult: f64) -> Self {
        let upper_thresh = db_to_gain(params.upper_thresh_db);
        let lower_thresh = db_to_gain(params.lower_thresh_db);
        let mut s = Self {
            params,
            env_attack_samples: 5.0,
            env_release_samples: 5.0,
            attack_scale: 1.0 / 6.0,
            release_scale: 1.0 / 6.0,
            upper_thresh_sq: upper_thresh * upper_thresh,
            lower_thresh_sq: lower_thresh * lower_thresh,
            upper_ratio_pow: params.upper_ratio * 0.5,
            lower_ratio_pow: params.lower_ratio * 0.5,
            output_mult: db_to_gain(params.output_gain_db),
            high_env_sq: 0.0,
            low_env_sq: 0.0,
        };
        s.set_sample_rate(sample_rate, time_mult);
        s
    }

    fn set_sample_rate(&mut self, sample_rate: f64, time_mult: f64) {
        let samples_per_ms = sample_rate / 1000.0;
        self.env_attack_samples =
            (self.params.base_attack_ms * time_mult * samples_per_ms).max(K_MIN_SAMPLE_ENVELOPE);
        self.env_release_samples =
            (self.params.base_release_ms * time_mult * samples_per_ms).max(K_MIN_SAMPLE_ENVELOPE);
        self.attack_scale = 1.0 / (self.env_attack_samples + 1.0);
        self.release_scale = 1.0 / (self.env_release_samples + 1.0);
    }

    #[inline]
    fn reset(&mut self) {
        self.high_env_sq = 0.0;
        self.low_env_sq = 0.0;
    }

    #[inline]
    fn process(&mut self, sample: f64, depth: f64) -> f64 {
        let sample_sq = sample * sample;

        if sample_sq > self.high_env_sq {
            self.high_env_sq =
                (sample_sq + self.high_env_sq * self.env_attack_samples) * self.attack_scale;
        } else {
            self.high_env_sq =
                (sample_sq + self.high_env_sq * self.env_release_samples) * self.release_scale;
        }
        self.high_env_sq = self.high_env_sq.max(self.upper_thresh_sq);
        let upper_mult = (self.upper_thresh_sq / self.high_env_sq).powf(self.upper_ratio_pow);

        if sample_sq > self.low_env_sq {
            self.low_env_sq =
                (sample_sq + self.low_env_sq * self.env_attack_samples) * self.attack_scale;
        } else {
            self.low_env_sq =
                (sample_sq + self.low_env_sq * self.env_release_samples) * self.release_scale;
        }
        self.low_env_sq = self.low_env_sq.min(self.lower_thresh_sq).max(1e-24);
        let lower_mult = (self.lower_thresh_sq / self.low_env_sq).powf(self.lower_ratio_pow);

        let gain_compression = (upper_mult * lower_mult).clamp(0.0, K_MAX_EXPAND_MULT);
        let wet = gain_compression * sample * self.output_mult;
        sample + depth * (wet - sample)
    }
}

/// Mono 3-band OTT processor (instantiate one per channel for stereo).
#[derive(Debug, Clone, Copy)]
pub struct OttMono {
    depth: f64,
    time_mult: f64,
    low_mid_xover: LinkwitzRiley4,
    low_phase_align: LinkwitzRiley4,
    mid_high_xover: LinkwitzRiley4,
    low_comp: OttBandCompressor,
    mid_comp: OttBandCompressor,
    high_comp: OttBandCompressor,
}

impl OttMono {
    pub fn new(sample_rate: f64, depth: f64, time_mult: f64) -> Self {
        let low_params = BandParams {
            base_attack_ms: 2.8,
            base_release_ms: 40.0,
            lower_thresh_db: -35.0,
            upper_thresh_db: -28.0,
            lower_ratio: 0.8,
            upper_ratio: 0.9,
            output_gain_db: 16.3,
        };
        let mid_params = BandParams {
            base_attack_ms: 1.4,
            base_release_ms: 28.0,
            lower_thresh_db: -36.0,
            upper_thresh_db: -25.0,
            lower_ratio: 0.8,
            upper_ratio: 0.857,
            output_gain_db: 11.7,
        };
        let high_params = BandParams {
            base_attack_ms: 0.7,
            base_release_ms: 15.0,
            lower_thresh_db: -35.0,
            upper_thresh_db: -30.0,
            lower_ratio: 0.8,
            upper_ratio: 1.0,
            output_gain_db: 16.3,
        };

        Self {
            depth: depth.clamp(0.0, 1.0),
            time_mult,
            low_mid_xover: LinkwitzRiley4::new(120.0, sample_rate),
            low_phase_align: LinkwitzRiley4::new(2500.0, sample_rate),
            mid_high_xover: LinkwitzRiley4::new(2500.0, sample_rate),
            low_comp: OttBandCompressor::new(low_params, sample_rate, time_mult),
            mid_comp: OttBandCompressor::new(mid_params, sample_rate, time_mult),
            high_comp: OttBandCompressor::new(high_params, sample_rate, time_mult),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.low_mid_xover.set_sample_rate(sample_rate);
        self.low_phase_align.set_sample_rate(sample_rate);
        self.mid_high_xover.set_sample_rate(sample_rate);
        self.low_comp.set_sample_rate(sample_rate, self.time_mult);
        self.mid_comp.set_sample_rate(sample_rate, self.time_mult);
        self.high_comp.set_sample_rate(sample_rate, self.time_mult);
    }

    #[inline]
    pub fn reset(&mut self) {
        self.low_mid_xover.reset();
        self.low_phase_align.reset();
        self.mid_high_xover.reset();
        self.low_comp.reset();
        self.mid_comp.reset();
        self.high_comp.reset();
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let x = input as f64;
        let (low_raw, mid_high) = self.low_mid_xover.split(x);
        let (low_lp, low_hp) = self.low_phase_align.split(low_raw);
        let low = low_lp + low_hp;
        let (mid, high) = self.mid_high_xover.split(mid_high);

        let low_out = self.low_comp.process(low, self.depth);
        let mid_out = self.mid_comp.process(mid, self.depth);
        let high_out = self.high_comp.process(high, self.depth);

        (low_out + mid_out + high_out) as f32
    }
}
