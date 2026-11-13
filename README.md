# J-SAT

A zero-allocation multi-stage "Phat Rack" VST3 and CLAP audio plugin written in Rust using `nih-plug` and `nih_plug_vizia`.

---

## Documentation

- 📖 **[User Guide (`docs/USER_GUIDE.md`)](./docs/USER_GUIDE.md)** — Walkthrough of every knob, toggle, keyboard shortcut, and gain-staging tips.
- 📐 **[DSP Architecture & Math (`docs/DSP.md`)](./docs/DSP.md)** — Transfer functions, 4x polyphase IIR oversampling, 3-band Linkwitz-Riley OTT dynamics, and Drive auto-gain equations.

---

## Building & Installing the Plugin

### Building Locally from Source
```bash
# Run unit tests
cargo test --workspace

# Build and bundle release VST3 and CLAP plugins
cargo xtask bundle j-sat --release
```

The compiled bundles are written to `target/bundled/j-sat.vst3` and `target/bundled/j-sat.clap`.

### Installing & Bypassing macOS Gatekeeper
On **macOS**, copy `j-sat.vst3` to `/Library/Audio/Plug-Ins/VST3/` (system-wide) or `~/Library/Audio/Plug-Ins/VST3/` (current user), and `j-sat.clap` to `/Library/Audio/Plug-Ins/CLAP/` or `~/Library/Audio/Plug-Ins/CLAP/`.

Because local builds are not notarized with an Apple Developer ID certificate, strip quarantine attributes and apply an ad-hoc signature if macOS Gatekeeper flags the bundles:

```bash
sudo cp -R target/bundled/j-sat.vst3 /Library/Audio/Plug-Ins/VST3/
sudo cp -R target/bundled/j-sat.clap /Library/Audio/Plug-Ins/CLAP/

sudo xattr -cr /Library/Audio/Plug-Ins/VST3/j-sat.vst3 /Library/Audio/Plug-Ins/CLAP/j-sat.clap
sudo codesign --force --deep --sign - /Library/Audio/Plug-Ins/VST3/j-sat.vst3
sudo codesign --force --deep --sign - /Library/Audio/Plug-Ins/CLAP/j-sat.clap
```

On **Windows**, copy `j-sat.vst3` to `C:\Program Files\Common Files\VST3\` and `j-sat.clap` to `C:\Program Files\Common Files\CLAP\`.
On **Linux**, copy them to `~/.vst3/` and `~/.clap/`.

---

## DSP Highlights

1. **Dual-Stage 3-Band OTT Compression (`OttMono`):**
   - Stage 1 (**Subtle Glue**, `Depth = 33%`, `Time = 1000%`) and Stage 2 (**Full OTT**, `Depth = 100%`, `Time = 1000%`) using 4th-order phase-aligned Linkwitz-Riley crossovers (`120 Hz` and `2500 Hz`) with upward/downward RMS envelope followers.
2. **4x Oversampled Nonlinear Stages (`Oversampler4x`):**
   - Fixed `+10 dB` kHs-style `tanh` Saturate stage driving hot into OTT Stage 2, followed by a 1-pole `7.5 Hz` DC blocker.
   - Cascaded CamelCrusher-style **Mech** (200 Hz low-shelf + 4 kHz high-shelf pre-emphasis EQ + cubic-softened hard clipper) $\rightarrow$ **Tube** (asymmetric biased `tanh`) waveshaper with a concave `t^1.4` Drive taper (`-40 dB` to `-14 dB`) and `-0.35 dB/dB` downward auto-gain compensation.
3. **Tone Shaping, Sub Cut & 1x Output Clipper:**
   - `6000 Hz` (`Q = 0.707`) Treble high shelf (`-6 dB` to `+6 dB`), parallel harshness-taming notches (`2000 Hz` and `3200 Hz`, `-10 dB`), toggleable `135 Hz` 48 dB/oct 8th-order Butterworth Sub Cut, and a 1x-rate `-3.0 dBFS` quadratic soft-knee / `0.0 dBFS` hard output clipper.

---

## Workspace Structure

```text
J-SAT/
├── .cargo/config.toml        # `cargo xtask` alias
├── Cargo.toml                # Workspace manifest
├── crates/
│   ├── j-sat-dsp/            # Pure, zero-allocation Rust DSP library
│   │   └── src/
│   │       ├── lib.rs        # 9-stage stereo JSatProcessor + JSatParams
│   │       ├── biquad.rs     # 64-bit RBJ biquads & 1-pole DC blocker
│   │       ├── oversample.rs # 4x polyphase IIR half-band oversampler
│   │       ├── ott.rs        # 3-band Linkwitz-Riley OTT compressor
│   │       └── distortion.rs # Saturate, Mech -> Tube cascade, Soft/Hard clipper
│   └── j-sat/                # "J-SAT" VST3/CLAP plugin & skeuomorphic Vizia GUI
├── docs/
│   ├── USER_GUIDE.md         # Full parameter & usage guide
│   └── DSP.md                # Mathematical derivations & signal-chain architecture
└── xtask/                    # `nih_plug_xtask` bundler
```

---

## License & Acknowledgements

Licensed under the **GNU General Public License v3.0 or later (GPL-3.0-or-later)**.

- **[`nih-plug`](https://github.com/robbert-vdh/nih-plug)** & **`nih_plug_vizia`** by Robbert van der Helm for the Rust VST3/CLAP plugin framework and Vizia GUI integration.
- **[`vitOTTx`](https://github.com/RustedYeast/vitOTTx)** (derived from Matt Tytel's Vital synth DSP) for the 3-band Linkwitz-Riley OTT crossover and upward/downward envelope follower math.
- **[`CrushClone`](https://github.com/doxiedevops/CrushClone)** for the Mech and Tube distortion waveshaping curves.

---

## Disclaimer

This plugin was developed with heavy use of AI-assisted coding ("vibecoding"). While tested for audio safety and stability, use it at your own risk—always keep a limiter on your master bus when experimenting with extreme settings. Kilohearts, Snap Heap, and CamelCrusher are trademarks of their respective owners; J-SAT is an independent open-source project and is not affiliated with or endorsed by them.
