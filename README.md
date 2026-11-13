# J-SAT

A zero-allocation multi-stage "Phat Rack" VST3 and CLAP audio plugin written in Rust using `nih-plug` and `nih_plug_vizia`.

---

## Signal Chain (Strict Processing Order)

1. **Pre-Gain** (`-30.0 dB` to `0.0 dB`, default `0.0 dB`) — Smooth linear gain staging into the rack.
2. **OTT Stage 1 (Subtle Glue)** — 3-band Linkwitz-Riley crossover (`120 Hz` / `2500 Hz`) + upward/downward RMS envelope followers (`Depth = 33%`, `Time = 1000%`).
3. **Saturate Stage** — kHs-style `+10 dB` soft-saturating `tanh` waveshaper (4x polyphase IIR half-band oversampled) followed by a 1-pole DC blocker (`7.5 Hz`).
4. **OTT Stage 2 (Full OTT)** — Identical 3-band OTT topology (`Depth = 100%`, `Time = 1000%`).
5. **Treble High Shelf** (`-6.0 dB` to `+6.0 dB`, default `0.0 dB`) — 2-pole RBJ high shelf at `6000 Hz`, `Q = 0.707`.
6. **Dual Waveshaper Cascade (Mech → Tube)** — 4x polyphase IIR half-band oversampled CamelCrusher-style **Mech** (200 Hz low-shelf + 4 kHz high-shelf pre-emphasis EQ + cubic-softened hard clipper) cascaded directly into **Tube** (asymmetric `tanh` with bias DC compensation), driven by **Drive** (`0%` to `100%`).
7. **Harshness Taming Peak Filters (Parallel Mix)** — Two cascaded RBJ peaking notches (`2000 Hz`, `-10 dB`, `Q = 2.42` and `3200 Hz`, `-10 dB`, `Q = 5.87`) blended via **Tame** (`0%` to `100%`, default `0.0%`).
8. **Sub Cut High-Pass Filter** — Toggleable 8th-order (`48 dB/oct`) Butterworth high-pass filter at `135 Hz` (default `OFF`).
9. **Output Clipper (1x Native Rate)** — **Soft Clip** (`ON` by default): C1-smooth `-3.0 dBFS` quadratic soft-knee clipper; (`OFF`): strict `0.0 dBFS` hard clipper.

---

## Building & Installing

### Running Tests & Bundling VST3 / CLAP
```bash
# Run unit tests
cargo test --workspace

# Build and bundle release VST3 and CLAP plugins
cargo xtask bundle j-sat --release
```

The compiled bundles are written to `target/bundled/j-sat.vst3` and `target/bundled/j-sat.clap`.

### Installing on macOS (System-Wide)
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

## Workspace Structure

```text
J-SAT/
├── .cargo/config.toml        # `cargo xtask` alias
├── Cargo.toml                # Workspace manifest
├── crates/
│   ├── j-sat-dsp/            # Pure, zero-allocation Rust DSP library
│   └── j-sat/                # VST3/CLAP plugin & skeuomorphic Vizia GUI
└── xtask/                    # `nih_plug_xtask` bundler
```

---

## License & Acknowledgements

Licensed under the **GNU General Public License v3.0 or later (GPL-3.0-or-later)**.

- **OTT Multiband Compressor DSP** ported from [`vitOTTx`](https://github.com/RustedYeast/vitOTTx) (based on Matt Tytel's Vital synth DSP).
- **Mech & Tube Waveshapers** ported from [`CrushClone`](https://github.com/doxiedevops/CrushClone).
- **Plugin Framework & GUI Architecture** built with [`nih-plug`](https://github.com/robbert-vdh/nih-plug) and modeled on [`tube-resonator`](https://github.com/jakehukkanen/tube-resonator).
