/// Audio events that game systems can emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEvent {
    NoticeChime,
    UrgentAttention,
    CriticalAlert,
    GolfStroke,
    GolfBallSink,
    PlatformerJump,
    PlatformerPowerUp,
    PlatformerFinish,
    LaserFire,
    LaserHit,
    TronCrash,
    TronGrind,
    TronWin,
}

/// Queue of audio events to be processed each frame.
#[derive(Default)]
pub struct AudioEventQueue {
    events: Vec<AudioEvent>,
}

impl AudioEventQueue {
    pub fn push(&mut self, event: AudioEvent) {
        self.events.push(event);
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn process(&mut self, manager: &AudioManager, settings: &AudioSettings) {
        for event in self.events.drain(..) {
            let (freq, dur, wave, vol_category) = match event {
                AudioEvent::NoticeChime => (440.0, 0.15, WaveType::Sine, SoundCategory::Overlay),
                AudioEvent::UrgentAttention => {
                    (330.0, 0.25, WaveType::Triangle, SoundCategory::Overlay)
                },
                AudioEvent::CriticalAlert => (220.0, 0.4, WaveType::Square, SoundCategory::Overlay),
                AudioEvent::GolfStroke => (250.0, 0.1, WaveType::Sine, SoundCategory::Game),
                AudioEvent::GolfBallSink => (520.0, 0.3, WaveType::Sine, SoundCategory::Game),
                AudioEvent::PlatformerJump => {
                    (330.0, 0.08, WaveType::Triangle, SoundCategory::Game)
                },
                AudioEvent::PlatformerPowerUp => (440.0, 0.2, WaveType::Sine, SoundCategory::Game),
                AudioEvent::PlatformerFinish => {
                    (520.0, 0.5, WaveType::Triangle, SoundCategory::Game)
                },
                AudioEvent::LaserFire => (280.0, 0.06, WaveType::Sawtooth, SoundCategory::Game),
                AudioEvent::LaserHit => (180.0, 0.15, WaveType::Square, SoundCategory::Game),
                AudioEvent::TronCrash => (200.0, 0.3, WaveType::Square, SoundCategory::Game),
                AudioEvent::TronGrind => (350.0, 0.05, WaveType::Sawtooth, SoundCategory::Game),
                AudioEvent::TronWin => (520.0, 0.5, WaveType::Triangle, SoundCategory::Game),
            };

            let category_vol = match vol_category {
                SoundCategory::Game => settings.game_volume,
                SoundCategory::Overlay => settings.overlay_volume,
            };
            let final_vol = settings.master_volume * category_vol;

            if final_vol > 0.001 {
                manager.play_tone(freq, dur, final_vol, wave);
            }
        }
    }
}

/// Audio settings.
#[derive(Clone, PartialEq)]
pub struct AudioSettings {
    pub master_volume: f32,
    pub game_volume: f32,
    pub overlay_volume: f32,
    pub music_volume: f32,
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.5,
            game_volume: 0.7,
            overlay_volume: 0.8,
            music_volume: 0.3,
            muted: false,
        }
    }
}

/// Wrapping the Web Audio API context.
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

impl Default for AudioManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WaveType {
    Sine,
    Square,
    Triangle,
    Sawtooth,
}

enum SoundCategory {
    Game,
    Overlay,
}
