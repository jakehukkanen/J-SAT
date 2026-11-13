//! `j-sat`: Zero-allocation VST3 and CLAP multi-stage "Phat Rack" audio plugin.

use std::num::NonZeroU32;
use std::sync::Arc;

use j_sat_dsp::{ClipMode, JSatParams, JSatProcessor, ScopeRing};
use nih_plug::prelude::*;
use nih_plug_vizia::ViziaState;

mod editor;

pub struct JSatPlugin {
    params: Arc<JSatPluginParams>,
    dsp: JSatProcessor,
    scope: Arc<ScopeRing>,
}

#[derive(Params)]
pub struct JSatPluginParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<ViziaState>,

    #[id = "pre_gain"]
    pub pre_gain_db: FloatParam,

    #[id = "drive"]
    pub drive: FloatParam,

    #[id = "treble"]
    pub treble_db: FloatParam,

    #[id = "tame"]
    pub notch_mix: FloatParam,

    #[id = "sub_cut"]
    pub sub_cut: BoolParam,

    #[id = "clip_gain"]
    pub clip_gain_db: FloatParam,

    #[id = "soft_clip"]
    pub soft_clip: BoolParam,
}

impl Default for JSatPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(JSatPluginParams::default()),
            dsp: JSatProcessor::new(44100.0),
            scope: Arc::new(ScopeRing::new()),
        }
    }
}

impl Default for JSatPluginParams {
    fn default() -> Self {
        let def = JSatParams::default();
        Self {
            editor_state: editor::default_state(),

            pre_gain_db: FloatParam::new(
                "Pre-Gain",
                def.pre_gain_db,
                FloatRange::Linear {
                    min: -30.0,
                    max: 0.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            drive: FloatParam::new(
                "Drive",
                def.drive,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(1))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            treble_db: FloatParam::new(
                "Treble",
                def.treble_db,
                FloatRange::Linear {
                    min: -6.0,
                    max: 6.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            notch_mix: FloatParam::new(
                "Tame",
                def.notch_mix,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(1))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            sub_cut: BoolParam::new("Sub Cut", def.sub_cut),

            clip_gain_db: FloatParam::new(
                "Clip Gain",
                def.clip_gain_db,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            soft_clip: BoolParam::new("Soft Clip", true),
        }
    }
}

impl Plugin for JSatPlugin {
    const NAME: &'static str = "J-SAT";
    const VENDOR: &'static str = "Jake Hukkanen";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(
            self.params.clone(),
            self.params.editor_state.clone(),
            self.scope.clone(),
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.dsp.set_sample_rate(buffer_config.sample_rate as f64);
        self.dsp.reset();
        true
    }

    fn reset(&mut self) {
        self.dsp.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let num_samples = buffer.samples();
        let channels = buffer.as_slice();
        if channels.len() < 2 {
            return ProcessStatus::Normal;
        }

        let (left_slice, right_rest) = channels.split_at_mut(1);
        let left = &mut left_slice[0][..num_samples];
        let right = &mut right_rest[0][..num_samples];

        const SUB_BLOCK: usize = 32;
        let mut offset = 0;
        while offset < num_samples {
            let len = (num_samples - offset).min(SUB_BLOCK);
            let step = len as u32;

            let dsp_params = JSatParams {
                pre_gain_db: self.params.pre_gain_db.smoothed.next_step(step),
                drive: self.params.drive.smoothed.next_step(step),
                treble_db: self.params.treble_db.smoothed.next_step(step),
                notch_mix: self.params.notch_mix.smoothed.next_step(step),
                sub_cut: self.params.sub_cut.value(),
                clip_gain_db: self.params.clip_gain_db.smoothed.next_step(step),
                clip_mode: if self.params.soft_clip.value() {
                    ClipMode::Soft
                } else {
                    ClipMode::Hard
                },
            };

            self.dsp.process_block_with_scope(
                &mut left[offset..offset + len],
                &mut right[offset..offset + len],
                &dsp_params,
                Some(&self.scope),
            );
            offset += len;
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for JSatPlugin {
    const CLAP_ID: &'static str = "com.jakehukkanen.j-sat";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Multi-stage Phat Rack saturation, OTT, EQ, and clipper");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Distortion,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for JSatPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"JH_JSatPhatRack1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Distortion,
        Vst3SubCategory::Dynamics,
    ];
}

nih_export_clap!(JSatPlugin);
nih_export_vst3!(JSatPlugin);
