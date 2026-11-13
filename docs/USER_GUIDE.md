# J-SAT — User Guide

**J-SAT** is a zero-allocation multi-stage "Phat Rack" saturation, multiband dynamics, tone-shaping, and clipping plugin (VST3 & CLAP). It packs an entire aggressive bass/synth processing chain—glue OTT, hot saturation, full OTT, high-shelf air, cascaded Mech → Tube distortion, harshness notches, sub cut, and output clipping—into six focused controls.

---

## Mouse & Keyboard Shortcuts

| Action | Behavior |
| :--- | :--- |
| **Click + Drag Vertically** | Adjust knob value |
| **Shift + Drag Vertically** | Fine / high-precision adjustment |
| **Cmd + Click** (macOS) / **Ctrl + Click** (Win/Linux) | Reset control to its default value |
| **Double-Click** or **Right-Click** | Reset control to its default value |
| **Mouse Scroll Wheel / Trackpad** | Step parameter up/down (`Shift + Scroll` for fine steps) |

---

## Left Section — Input & Saturation

### 1. `PRE-GAIN` (`-30.0 dB` to `0.0 dB`, default `0.0 dB`)
Attenuates the input signal smoothly before it enters OTT Stage 1 and the `+10 dB` Saturate stage.
- Because both OTT stages and Saturate react strongly to input level, pulling `PRE-GAIN` down (`-6 dB` to `-18 dB`) lets you back off the initial OTT upward/downward pumping and saturation density on hot synth or drum busses without changing your DAW's fader.

### 2. `DRIVE` (`0.0%` to `100.0%`, default `30.0%`)
Simultaneously drives the **Mech → Tube** waveshaper cascade (positioned after OTT Stage 2 and the 6 kHz Treble shelf).
- **Non-Linear Taper & Sweet Spot:** `DRIVE` uses a concave `t^1.4` curve mapping `0%` to `-40.0 dB` and `100%` to `-14.0 dB` into the Mech pre-emphasis and clipper. The default `30.0%` (`~ -35.2 dB` pre-trim) sits right in the sweet spot for warm harmonic saturation without premature squashing, while the upper half of the knob pushes into dense, aggressive Mech/Tube distortion.
- **Downward Auto-Gain Compensation:** At `0.0%`, post-Tube compensation is `0.0 dB` (preserving clean dynamics). As `DRIVE` increases, output gain scales down smoothly at `-0.35 dB` per dB of drive (reaching `-9.1 dB` at `100%`) so turning up `DRIVE` adds saturation density rather than just blasting volume into the output clipper.

---

## Right Section — Tone & Output

### 3. `TREBLE` (`-6.0 dB` to `+6.0 dB`, default `0.0 dB`)
Bipolar rotary knob controlling a `6000 Hz` (`Q = 0.707`) 2-pole RBJ high-shelf filter placed **before** the Mech → Tube waveshaper.
- Boosting `TREBLE` (`+2` to `+6 dB`) pushes high-frequency sizzle and air into the Mech/Tube distortion for bright, modern neuro/bass presence; cutting (`-2` to `-6 dB`) darkens and smooths the distortion harmonics.

### 4. `TAME` (`0.0%` to `100.0%`, default `0.0%`)
Parallel wet/dry blend for two cascaded harshness-taming peaking notches (`2000 Hz` at `-10.0 dB`, `Q = 2.42` and `3200 Hz` at `-10.0 dB`, `Q = 5.87`) placed right after Mech → Tube.
- At `0.0%`, the notches are bypassed for raw, forward midrange bite.
- Turning `TAME` up toward `50%–100%` carves out fatiguing 2–3.2 kHz ear-piercing resonances—especially useful at high `DRIVE` or `TREBLE` settings.

### 5. `SUB CUT` (LED Toggle, default `OFF`)
Engages an 8th-order (`48 dB/oct`) Butterworth high-pass filter at `135 Hz`.
- Toggle **ON** when processing mid-bass layers, growls, or guitars/leads that need to stay completely clear of a separate sub-bass channel.
- Leave **OFF** for full-range basses, kicks, or drum busses.

### 6. `SOFT CLIP` (LED Toggle, default `ON`)
Selects the final `1x` native-rate output clipper at `0.0 dBFS`:
- **ON (Default):** `-3.0 dBFS` quadratic soft-knee clipper that smoothly rounds transient peaks into `0.0 dBFS` for loud, punchy analog-style limiting.
- **OFF:** Strict `0.0 dBFS` digital hard clipper (`[-1.0, 1.0]`) for maximum aggressive transient crunch.
