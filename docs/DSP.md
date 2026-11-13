# J-SAT DSP Architecture & Mathematical Model

This document details the zero-allocation digital signal processing (DSP) pipeline implemented in [`crates/j-sat-dsp`](../crates/j-sat-dsp). All filter states, oversamplers, and envelope followers are fixed-size stack/struct members initialized in `initialize()` / `Default`, with `assert_process_allocs` enabled on the plugin target to guarantee zero heap allocations on the audio thread.

---

## Signal Chain Overview

Stereo audio is processed in 32-sample sub-blocks (for sample-accurate parameter smoothing) through nine strict stages in series:

```text
Input -> [1. Pre-Gain] -> [2. OTT 33%] -> [3. Saturate +10dB (4x OS) + DC Block]
      -> [4. OTT 100%] -> [5. 6kHz High Shelf] -> [6. Mech->Tube (4x OS) + AutoGain]
      -> [7. Parallel Harshness Notches] -> [8. 135Hz 48dB/oct Sub Cut]
      -> [9. Clip Gain + 1x Clipper] -> Output
```

---

## 1. Pre-Gain
Converts `pre_gain_db` ($\in [-30.0, 0.0]\text{ dB}$) to linear amplitude with 10 ms linear parameter smoothing:

$$g_{\text{pre}} = 10^{\,\text{pre\_gain\_db} / 20}$$

---

## 2. OTT Stage 1 & Stage 2 (`OttMono`)

Both Stage 2 (**Depth = 33%**, **Time = 1000%**) and Stage 4 (**Depth = 100%**, **Time = 1000%**) use identical 3-band OTT topologies ported from `vitOTTx` (Vital DSP), instantiated independently per stereo channel.

### 2.1 Phase-Aligned 3-Band Linkwitz-Riley Crossover (`LinkwitzRiley4`)
Splits the signal at **$f_{\text{LM}} = 120\text{ Hz}$** and **$f_{\text{MH}} = 2500\text{ Hz}$** using 4th-order Linkwitz-Riley filters (two cascaded 2nd-order Butterworth LP/HP pairs, $Q = 1/\sqrt{2}$). To keep all three bands phase-aligned upon summation, the low band from the $120\text{ Hz}$ split is passed through a matching $2500\text{ Hz}$ LP+HP allpass pair before compression.

### 2.2 Upward / Downward RMS Envelope Followers
Each band tracks squared signal power $x^2[n]$ with separate attack and release time constants scaled by $\text{Time} = 1000\%$ ($10\times$ base times):

| Band | Attack ($\times 10$) | Release ($\times 10$) | Lower Thresh | Upper Thresh | Lower Ratio | Upper Ratio | Makeup Gain |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Low** (`<120 Hz`) | `28.0 ms` | `400.0 ms` | `-35.0 dB` | `-28.0 dB` | `0.8` | `0.9` | `+16.3 dB` |
| **Mid** (`120–2500 Hz`) | `14.0 ms` | `280.0 ms` | `-36.0 dB` | `-25.0 dB` | `0.8` | `0.857` | `+11.7 dB` |
| **High** (`>2500 Hz`) | `7.0 ms` | `150.0 ms` | `-35.0 dB` | `-30.0 dB` | `0.8` | `1.0` | `+16.3 dB` |

The gain multiplier for each band combines downward compression above the upper threshold and upward expansion (clamped at $32\times$) below the lower threshold, blended with the dry band via `depth` ($0.33$ in Stage 1, $1.00$ in Stage 2).

---

## 3. Saturate Stage & 4x Polyphase IIR Oversampling (`Oversampler4x`)

### 3.1 4x Polyphase IIR Half-Band Oversampler
Nonlinear stages (Stage 3 and Stage 6) are wrapped in two cascaded $2\times$ elliptic polyphase allpass half-band stages:
- **Stage 1 ($1\times \leftrightarrow 2\times$):** 12th-order (6 coefficients per branch), $>70\text{ dB}$ stopband attenuation, $<0.000001\text{ dB}$ passband ripple.
- **Stage 2 ($2\times \leftrightarrow 4\times$):** 6th-order (3 coefficients per branch), $>90\text{ dB}$ stopband attenuation.

### 3.2 kHs-Style `+10 dB` Saturate + DC Blocker
At the $4\times$ rate, audio is driven by $+10\text{ dB}$ ($g = 10^{10/20} \approx 3.16227766$) through a symmetric hyperbolic tangent waveshaper:

$$y[n] = \tanh(3.16227766 \cdot x[n])$$

with no output attenuation so it drives hot directly into OTT Stage 2. Immediately after downsampling, a 1-pole DC blocker ($f_c = 7.5\text{ Hz}$) removes sub-audio DC offset:

$$y_{\text{dc}}[n] = x[n] - x[n - 1] + R \cdot y_{\text{dc}}[n - 1], \qquad R = e^{-2\pi \cdot 7.5 / f_s}$$

---

## 4. Treble High Shelf
A 64-bit Direct Form II Transposed 2-pole RBJ high-shelf biquad at **$f_0 = 6000\text{ Hz}$**, **$Q = 0.707$**, with gain controlled by `treble_db` ($\in [-6.0, +6.0]\text{ dB}$). Coefficients are recomputed only when `treble_db` changes.

---

## 5. Cascaded Mech → Tube Waveshapers & Auto-Gain (`MechTubeMono`)

Running inside a dedicated `Oversampler4x` at $4\times$ sample rate, normalized `DRIVE` ($t \in [0.0, 1.0]$, default `0.30`) maps through a concave power taper:

$$\text{drive\_db}(t) = -40.0 + t^{1.4} \cdot 26.0 \quad (\text{dB}) \qquad \Longrightarrow \qquad g_{\text{trim}} = 10^{\,\text{drive\_db} / 20}$$

1. **Mech Pre-Emphasis & Clipper (`mech_clip`):**
   - Low shelf (`200 Hz`, `Q = 0.7`, linear gain `1.8`) $\rightarrow$ High shelf (`4000 Hz`, `Q = 0.7`, linear gain `2.0`), computed at $4 f_s$.
   - Gain $g_m = 1 + 29 t$, clipped to $[-1, 1]$ with cubic softening $s - 0.15 t \cdot s^3$ inside $(-1, 1)$.
2. **Tube Saturator (`tube_saturate`):**
   - Gain $g_t = 1 + 19 t$ and asymmetric even-harmonic bias $b = 0.08 t$:
     $$y_{\text{tube}} = \tanh(g_t \cdot x + b) - \tanh(b)$$
3. **Downward-Only Auto-Gain Compensation:**
   - Anchored at $0.0\text{ dB}$ when $t = 0$:
     $$\text{comp\_db} = -0.35 \cdot (\text{drive\_db} - (-40.0)) \qquad \Longrightarrow \qquad g_{\text{comp}} = 10^{\,\text{comp\_db} / 20}$$

---

## 6. Harshness Taming Peak Filters (`Tame`)
Two cascaded 2-pole RBJ peaking EQ biquads run in parallel with the dry Stage 6 output:
- **Peak 1:** $f_0 = 2000\text{ Hz}$, $\text{Gain} = -10.0\text{ dB}$, $Q = 2.42$
- **Peak 2:** $f_0 = 3200\text{ Hz}$, $\text{Gain} = -10.0\text{ dB}$, $Q = 5.87$

Blended linearly via `notch_mix` ($m \in [0.0, 1.0]$, default `0.0`):

$$y = (1 - m) \cdot x_{\text{dry}} + m \cdot x_{\text{wet}}$$

---

## 7. Sub Cut High-Pass Filter
When `sub_cut` is enabled, audio passes through four cascaded 2-pole Butterworth high-pass biquads ($48\text{ dB/oct}$) at **$f_c = 135\text{ Hz}$** with 8th-order Butterworth pole $Q$ values:

$$Q = [\,0.5098,\; 0.6013,\; 0.8999,\; 2.5629\,]$$

Cleanly bypassed when disabled.

---

## 8. Clip Gain, 1x Output Clipper & Lock-Free Scope Telemetry
Immediately after `SUB CUT`, `clip_gain_db` ($\in [-12.0, +12.0]\text{ dB}$, 10 ms linear smoothing) scales the signal by $g_{\text{clip}} = 10^{\,\text{clip\_gain\_db} / 20}$ right before the clipper.

The clipper runs strictly at the native $1\times$ sample rate (no oversampling) so inter-sample reconstruction never exceeds `0.0 dBFS` (`[-1.0, 1.0]`):

- **Soft Clip (`ON`, default):** Linear below $T = 10^{-3/20} \approx 0.70794578$ (`-3.0 dBFS`), transitioning through a $C^1$-continuous quadratic knee up to $M = 2 - T \approx 1.2920542$:
  $$f(x) = \text{sgn}(x) \cdot \left(T + (|x| - T) - \frac{(|x| - T)^2}{4(1 - T)}\right), \qquad T < |x| < M$$
  and clamped to $\pm 1.0$ for $|x| \ge M$.
- **Hard Clip (`OFF`):** Strict digital clamp $\text{clamp}(x, -1.0, 1.0)$.

### Lock-Free Scope Ring Buffer (`ScopeRing`)
To drive the 60 fps Vizia `ClipperScope` without locks or audio-thread allocations, `JSatProcessor` downsamples the stereo max absolute peaks (`pre_clip` and `post_clip`) at a `~150 Hz` column rate (`(sample_rate / 150.0).round()` samples per column) and writes `f32::to_bits()` into a fixed 256-slot `[AtomicU32; 256]` ring buffer (`ScopeRing`) shared via `Arc`.
