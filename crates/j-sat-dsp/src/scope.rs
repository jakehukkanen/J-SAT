//! Zero-allocation lock-free ring buffer for audio-to-GUI clipper scope telemetry.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

pub const SCOPE_LEN: usize = 256;

/// Lock-free ring buffer storing pre-clip and post-clip peak amplitudes.
pub struct ScopeRing {
    pre_peaks: [AtomicU32; SCOPE_LEN],
    post_peaks: [AtomicU32; SCOPE_LEN],
    write_idx: AtomicUsize,
}

impl Default for ScopeRing {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeRing {
    pub fn new() -> Self {
        Self {
            pre_peaks: std::array::from_fn(|_| AtomicU32::new(0.0_f32.to_bits())),
            post_peaks: std::array::from_fn(|_| AtomicU32::new(0.0_f32.to_bits())),
            write_idx: AtomicUsize::new(0),
        }
    }

    #[inline]
    pub fn push(&self, pre_peak: f32, post_peak: f32) {
        let idx = self.write_idx.load(Ordering::Relaxed) % SCOPE_LEN;
        self.pre_peaks[idx].store(pre_peak.to_bits(), Ordering::Relaxed);
        self.post_peaks[idx].store(post_peak.to_bits(), Ordering::Relaxed);
        self.write_idx.store(idx.wrapping_add(1), Ordering::Release);
    }

    /// Copies the ring buffer chronologically (oldest -> newest) into `pre_out` and `post_out`.
    pub fn snapshot(&self, pre_out: &mut [f32; SCOPE_LEN], post_out: &mut [f32; SCOPE_LEN]) {
        let head = self.write_idx.load(Ordering::Acquire) % SCOPE_LEN;
        for i in 0..SCOPE_LEN {
            let idx = (head + i) % SCOPE_LEN;
            pre_out[i] = f32::from_bits(self.pre_peaks[idx].load(Ordering::Relaxed));
            post_out[i] = f32::from_bits(self.post_peaks[idx].load(Ordering::Relaxed));
        }
    }
}
