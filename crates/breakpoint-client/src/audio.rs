use bevy::prelude::*;

use crate::app::AppState;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioEventQueue>()
            .init_resource::<AudioSettings>()
            .insert_non_send_resource(AudioManager::new())
            .add_systems(
                Update,
                process_audio_events.run_if(not(resource_equals(AudioSettings {
                    muted: true,
                    ..AudioSettings::default()
                }))),
            )
            .add_systems(OnEnter(AppState::Lobby), load_audio_settings);
    }
}

/// Audio events that game systems can emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AudioEvent {
    // Overlay
    NoticeChime,
    UrgentAttention,
    CriticalAlert,

    // Golf
    GolfStroke,
    GolfBallSink,

    // Platformer
    PlatformerJump,
    PlatformerPowerUp,
    PlatformerFinish,

    // Laser Tag
    LaserFire,
    LaserHit,
}

/// Queue of audio events to be processed each frame.
#[derive(Resource, Default)]
pub struct AudioEventQueue {
    events: Vec<AudioEvent>,
}

impl AudioEventQueue {
    pub fn push(&mut self, event: AudioEvent) {
        self.events.push(event);
    }
}

/// Audio settings resource.
#[derive(Resource, Clone, PartialEq)]
pub struct AudioSettings {
    pub master_volume: f32,
    pub game_volume: f32,
    pub overlay_volume: f32,
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.5,
            game_volume: 0.7,
            overlay_volume: 0.8,
            muted: false,
        }
    }
}

/// Non-Send resource wrapping the Web Audio API context.
pub struct AudioManager {
    #[cfg(target_family = "wasm")]
    ctx: Option<web_sys::AudioContext>,
    #[cfg(not(target_family = "wasm"))]
    _phantom: (),
}

impl AudioManager {
    pub fn new() -> Self {
        Self {
            #[cfg(target_family = "wasm")]
            ctx: web_sys::AudioContext::new().ok(),
            #[cfg(not(target_family = "wasm"))]
            _phantom: (),
        }
    }

    /// Play a procedurally generated tone.
    #[allow(unused_variables)]
    pub fn play_tone(&self, frequency: f32, duration: f32, volume: f32, wave_type: WaveType) {
        #[cfg(target_family = "wasm")]
        {
            let Some(ctx) = &self.ctx else {
                return;
            };
            let Ok(oscillator) = ctx.create_oscillator() else {
                return;
            };
            let Ok(gain_node) = ctx.create_gain() else {
                return;
            };

            oscillator.set_type(match wave_type {
                WaveType::Sine => web_sys::OscillatorType::Sine,
                WaveType::Square => web_sys::OscillatorType::Square,
                WaveType::Triangle => web_sys::OscillatorType::Triangle,
                WaveType::Sawtooth => web_sys::OscillatorType::Sawtooth,
            });

            let _ = oscillator.frequency().set_value(frequency);
            let _ = gain_node.gain().set_value(volume);

            let now = ctx.current_time();
            // Envelope: quick attack, sustain, then release
            let _ = gain_node
                .gain()
                .linear_ramp_to_value_at_time(volume, now + 0.01);
            let _ = gain_node
                .gain()
                .linear_ramp_to_value_at_time(0.0, now + duration as f64);

            let _ = oscillator.connect_with_audio_node(&gain_node);
            let _ = gain_node.connect_with_audio_node(&ctx.destination());
            let _ = oscillator.start();
            let _ = oscillator.stop_with_when(now + duration as f64);
        }
    }
}

/// Oscillator wave types.
#[derive(Debug, Clone, Copy)]
pub enum WaveType {
    Sine,
    Square,
    Triangle,
    Sawtooth,
}

fn process_audio_events(
    mut queue: ResMut<AudioEventQueue>,
    settings: Res<AudioSettings>,
    audio: NonSend<AudioManager>,
) {
    if settings.muted {
        queue.events.clear();
        return;
    }

    for event in queue.events.drain(..) {
        let (freq, dur, wave, vol_category) = match event {
            // Overlay sounds
            AudioEvent::NoticeChime => (880.0, 0.15, WaveType::Sine, SoundCategory::Overlay),
            AudioEvent::UrgentAttention => {
                (660.0, 0.25, WaveType::Triangle, SoundCategory::Overlay)
            },
            AudioEvent::CriticalAlert => (440.0, 0.4, WaveType::Square, SoundCategory::Overlay),

            // Golf sounds
            AudioEvent::GolfStroke => (350.0, 0.1, WaveType::Sine, SoundCategory::Game),
            AudioEvent::GolfBallSink => (1200.0, 0.3, WaveType::Sine, SoundCategory::Game),

            // Platformer sounds
            AudioEvent::PlatformerJump => (500.0, 0.08, WaveType::Square, SoundCategory::Game),
            AudioEvent::PlatformerPowerUp => (900.0, 0.2, WaveType::Sine, SoundCategory::Game),
            AudioEvent::PlatformerFinish => (1000.0, 0.5, WaveType::Triangle, SoundCategory::Game),

            // Laser tag sounds
            AudioEvent::LaserFire => (1800.0, 0.06, WaveType::Sawtooth, SoundCategory::Game),
            AudioEvent::LaserHit => (200.0, 0.15, WaveType::Square, SoundCategory::Game),
        };

        let category_vol = match vol_category {
            SoundCategory::Game => settings.game_volume,
            SoundCategory::Overlay => settings.overlay_volume,
        };
        let final_vol = settings.master_volume * category_vol;

        if final_vol > 0.001 {
            audio.play_tone(freq, dur, final_vol, wave);
        }
    }
}

enum SoundCategory {
    Game,
    Overlay,
}

/// Load audio settings from localStorage on entering lobby.
fn load_audio_settings(mut settings: ResMut<AudioSettings>) {
    crate::storage::with_local_storage(|storage| {
        if let Ok(Some(val)) = storage.get_item("audio_muted") {
            settings.muted = val == "true";
        }
        if let Ok(Some(val)) = storage.get_item("audio_master_volume")
            && let Ok(v) = val.parse::<f32>()
        {
            settings.master_volume = v.clamp(0.0, 1.0);
        }
    });
}
