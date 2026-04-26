// Sound Engine — Synthesized 8-bit notification sounds via rodio
// Generates simple sine/square wave beeps for agent session events.

use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Sound events that map to agent lifecycle phases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundEvent {
    SessionStart,
    TaskComplete,
    TaskError,
    NeedsApproval,
    TaskConfirmation,
    ContextLimit,
}

/// Sound pack presets with different audio characteristics
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SoundPack {
    EightBit,
    Subtle,
}

/// Synthesized sound engine using rodio for audio output.
///
/// SAFETY: SoundEngine is created once on the main thread. The OutputStream and
/// OutputStreamHandle are !Send+!Sync due to cpal platform internals, but we only
/// ever use stream_handle to create Sink instances (which detach immediately).
/// All mutable state (volume, enabled) is behind Arc<Mutex>.
pub struct SoundEngine {
    /// Keep the stream alive for the lifetime of the engine
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    volume: Arc<Mutex<f32>>,
    enabled: Arc<Mutex<bool>>,
    /// Per-event enable/disable (all enabled by default)
    event_enabled: Arc<Mutex<std::collections::HashMap<SoundEvent, bool>>>,
    /// Active sound pack
    sound_pack: Arc<Mutex<SoundPack>>,
    /// Filter out sounds for probe/health-check sessions
    probe_filter: Arc<Mutex<bool>>,
}

// SAFETY: See doc comment on SoundEngine.
unsafe impl Send for SoundEngine {}
unsafe impl Sync for SoundEngine {}

impl SoundEngine {
    /// Create a new SoundEngine, initializing the audio output stream
    pub fn new() -> Option<Self> {
        match OutputStream::try_default() {
            Ok((stream, handle)) => Some(Self {
                _stream: stream,
                stream_handle: handle,
                volume: Arc::new(Mutex::new(0.7)),
                enabled: Arc::new(Mutex::new(true)),
                event_enabled: Arc::new(Mutex::new(std::collections::HashMap::new())),
                sound_pack: Arc::new(Mutex::new(SoundPack::EightBit)),
                probe_filter: Arc::new(Mutex::new(false)),
            }),
            Err(e) => {
                log::warn!("Failed to initialize audio output: {}", e);
                None
            }
        }
    }

    /// Set the master volume (0.0 to 1.0)
    pub fn set_volume(&self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        if let Ok(mut v) = self.volume.lock() {
            *v = volume;
        }
    }

    /// Enable or disable sound playback
    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut e) = self.enabled.lock() {
            *e = enabled;
        }
    }

    /// Get current enabled state
    pub fn is_enabled(&self) -> bool {
        self.enabled.lock().map(|e| *e).unwrap_or(false)
    }

    /// Enable or disable a specific sound event
    pub fn set_event_enabled(&self, event: SoundEvent, enabled: bool) {
        if let Ok(mut map) = self.event_enabled.lock() {
            map.insert(event, enabled);
        }
    }

    /// Check if a specific event is enabled (defaults to true)
    fn is_event_enabled(&self, event: SoundEvent) -> bool {
        self.event_enabled
            .lock()
            .map(|map| *map.get(&event).unwrap_or(&true))
            .unwrap_or(true)
    }

    /// Set the active sound pack
    pub fn set_sound_pack(&self, pack: SoundPack) {
        if let Ok(mut p) = self.sound_pack.lock() {
            *p = pack;
        }
    }

    /// Enable or disable probe session filtering
    pub fn set_probe_filter(&self, enabled: bool) {
        if let Ok(mut pf) = self.probe_filter.lock() {
            *pf = enabled;
        }
    }

    /// Check if probe filter is enabled
    pub fn is_probe_filter_enabled(&self) -> bool {
        self.probe_filter.lock().map(|pf| *pf).unwrap_or(false)
    }

    /// Play a sound event (non-blocking, spawns on a new sink)
    pub fn play(&self, event: SoundEvent) {
        if !self.is_enabled() || !self.is_event_enabled(event) {
            return;
        }

        let volume = self.volume.lock().map(|v| *v).unwrap_or(0.7);
        let pack = self.sound_pack.lock().map(|p| *p).unwrap_or(SoundPack::EightBit);

        let sink = match Sink::try_new(&self.stream_handle) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to create audio sink: {}", e);
                return;
            }
        };

        sink.set_volume(volume);

        match pack {
            SoundPack::EightBit => self.play_eight_bit(&sink, event),
            SoundPack::Subtle => self.play_subtle(&sink, event),
        }

        // Detach so it plays in the background without blocking
        sink.detach();
    }

    /// Eight-bit sound pack: square waves, punchy retro sounds (default)
    fn play_eight_bit(&self, sink: &Sink, event: SoundEvent) {
        match event {
            SoundEvent::SessionStart => {
                sink.append(square_wave(800.0, Duration::from_millis(150)));
            }
            SoundEvent::TaskComplete => {
                sink.append(sine_wave(600.0, Duration::from_millis(100)));
                sink.append(sine_wave(800.0, Duration::from_millis(100)));
            }
            SoundEvent::TaskError => {
                sink.append(sine_wave(400.0, Duration::from_millis(100)));
                sink.append(sine_wave(200.0, Duration::from_millis(100)));
            }
            SoundEvent::NeedsApproval => {
                sink.append(sine_wave(1000.0, Duration::from_millis(100)));
                sink.append(silence(Duration::from_millis(50)));
                sink.append(sine_wave(1000.0, Duration::from_millis(100)));
                sink.append(silence(Duration::from_millis(50)));
                sink.append(sine_wave(1000.0, Duration::from_millis(100)));
            }
            SoundEvent::TaskConfirmation => {
                sink.append(sine_wave(500.0, Duration::from_millis(60)));
                sink.append(sine_wave(700.0, Duration::from_millis(60)));
                sink.append(sine_wave(900.0, Duration::from_millis(60)));
            }
            SoundEvent::ContextLimit => {
                sink.append(square_wave(300.0, Duration::from_millis(200)));
            }
        }
    }

    /// Subtle sound pack: sine waves at lower volume with shorter duration
    fn play_subtle(&self, sink: &Sink, event: SoundEvent) {
        // Reduce volume by 40% for the subtle pack
        sink.set_volume(sink.volume() * 0.6);

        match event {
            SoundEvent::SessionStart => {
                sink.append(sine_wave(600.0, Duration::from_millis(80)));
            }
            SoundEvent::TaskComplete => {
                sink.append(sine_wave(500.0, Duration::from_millis(60)));
                sink.append(sine_wave(650.0, Duration::from_millis(60)));
            }
            SoundEvent::TaskError => {
                sink.append(sine_wave(350.0, Duration::from_millis(80)));
                sink.append(sine_wave(250.0, Duration::from_millis(80)));
            }
            SoundEvent::NeedsApproval => {
                sink.append(sine_wave(700.0, Duration::from_millis(60)));
                sink.append(silence(Duration::from_millis(40)));
                sink.append(sine_wave(700.0, Duration::from_millis(60)));
            }
            SoundEvent::TaskConfirmation => {
                sink.append(sine_wave(450.0, Duration::from_millis(40)));
                sink.append(sine_wave(600.0, Duration::from_millis(40)));
            }
            SoundEvent::ContextLimit => {
                sink.append(sine_wave(280.0, Duration::from_millis(120)));
            }
        }
    }
}

// ── Waveform generators ──────────────────────────────────────────

/// A sine wave source at a given frequency and duration
fn sine_wave(freq: f32, duration: Duration) -> SineWave {
    SineWave {
        freq,
        sample_rate: 44100,
        num_sample: 0,
        total_samples: (44100.0 * duration.as_secs_f32()) as usize,
    }
}

/// A square wave source at a given frequency and duration
fn square_wave(freq: f32, duration: Duration) -> SquareWave {
    SquareWave {
        freq,
        sample_rate: 44100,
        num_sample: 0,
        total_samples: (44100.0 * duration.as_secs_f32()) as usize,
    }
}

/// A silence source for gaps between beeps
fn silence(duration: Duration) -> Silence {
    Silence {
        sample_rate: 44100,
        num_sample: 0,
        total_samples: (44100.0 * duration.as_secs_f32()) as usize,
    }
}

// ── Sine wave iterator ───────────────────────────────────────────

struct SineWave {
    freq: f32,
    sample_rate: u32,
    num_sample: usize,
    total_samples: usize,
}

impl Iterator for SineWave {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.num_sample >= self.total_samples {
            return None;
        }
        let t = self.num_sample as f32 / self.sample_rate as f32;
        self.num_sample += 1;

        // Apply a simple envelope to avoid clicks
        let envelope = self.envelope();
        Some((t * self.freq * 2.0 * std::f32::consts::PI).sin() * 0.3 * envelope)
    }
}

impl SineWave {
    /// Simple attack/release envelope to avoid audio clicks
    fn envelope(&self) -> f32 {
        let attack_samples = (self.sample_rate as f32 * 0.005) as usize; // 5ms attack
        let release_samples = (self.sample_rate as f32 * 0.005) as usize; // 5ms release
        let release_start = self.total_samples.saturating_sub(release_samples);

        if self.num_sample < attack_samples {
            self.num_sample as f32 / attack_samples as f32
        } else if self.num_sample >= release_start {
            (self.total_samples - self.num_sample) as f32 / release_samples as f32
        } else {
            1.0
        }
    }
}

impl Source for SineWave {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.total_samples - self.num_sample)
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.total_samples as f32 / self.sample_rate as f32,
        ))
    }
}

// ── Square wave iterator ─────────────────────────────────────────

struct SquareWave {
    freq: f32,
    sample_rate: u32,
    num_sample: usize,
    total_samples: usize,
}

impl Iterator for SquareWave {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.num_sample >= self.total_samples {
            return None;
        }
        let t = self.num_sample as f32 / self.sample_rate as f32;
        self.num_sample += 1;

        // Square wave: sign of sine
        let envelope = self.envelope();
        let value = if (t * self.freq * 2.0 * std::f32::consts::PI).sin() >= 0.0 {
            0.2
        } else {
            -0.2
        };
        Some(value * envelope)
    }
}

impl SquareWave {
    fn envelope(&self) -> f32 {
        let attack_samples = (self.sample_rate as f32 * 0.005) as usize;
        let release_samples = (self.sample_rate as f32 * 0.005) as usize;
        let release_start = self.total_samples.saturating_sub(release_samples);

        if self.num_sample < attack_samples {
            self.num_sample as f32 / attack_samples as f32
        } else if self.num_sample >= release_start {
            (self.total_samples - self.num_sample) as f32 / release_samples as f32
        } else {
            1.0
        }
    }
}

impl Source for SquareWave {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.total_samples - self.num_sample)
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.total_samples as f32 / self.sample_rate as f32,
        ))
    }
}

// ── Silence source ───────────────────────────────────────────────

struct Silence {
    sample_rate: u32,
    num_sample: usize,
    total_samples: usize,
}

impl Iterator for Silence {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.num_sample >= self.total_samples {
            return None;
        }
        self.num_sample += 1;
        Some(0.0)
    }
}

impl Source for Silence {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.total_samples - self.num_sample)
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f32(
            self.total_samples as f32 / self.sample_rate as f32,
        ))
    }
}
