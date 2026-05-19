// Sound Engine — Synthesized 8-bit notification sounds via rodio
// Generates simple sine/square wave beeps for agent session events.

use chrono::Timelike;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SPAM_THRESHOLD: usize = 3;
const SPAM_WINDOW: Duration = Duration::from_secs(10);

/// Sound events that map to agent lifecycle phases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundEvent {
    SessionStart,
    SessionEnd,
    TaskComplete,
    TaskError,
    NeedsApproval,
    TaskConfirmation,
    PlanApproval,
    ContextLimit,
    Boot,
}

impl SoundEvent {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "session-start" | "session_start" => Some(Self::SessionStart),
            "sessionStart" => Some(Self::SessionStart),
            "session-end" | "session_end" => Some(Self::SessionEnd),
            "session-error" | "session_error" | "error" => Some(Self::TaskError),
            "permission-request" | "permission_request" | "permission" => Some(Self::NeedsApproval),
            "question-asked" | "question_asked" | "question" => Some(Self::TaskConfirmation),
            "task-complete" | "task_complete" | "complete" => Some(Self::TaskComplete),
            "plan-approval" | "plan_approval" | "plan" => Some(Self::PlanApproval),
            "resource" => Some(Self::ContextLimit),
            "context-compact" | "context_compact" | "context-limit" | "context_limit" => {
                Some(Self::ContextLimit)
            }
            "token-limit" | "token_limit" => Some(Self::ContextLimit),
            "boot" => Some(Self::Boot),
            _ => None,
        }
    }
}

/// Sound pack presets with different audio characteristics
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SoundPack {
    EightBit,
    Subtle,
    Synth,
    System,
    None,
}

impl SoundPack {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "eight-bit" | "8bit" => Some(Self::EightBit),
            "subtle" => Some(Self::Subtle),
            "synth" => Some(Self::Synth),
            "system" => Some(Self::System),
            "none" => Some(Self::None),
            // Custom per-event audio is not available in the Tauri backend yet;
            // keep playback enabled with the closest built-in pack.
            "custom" => Some(Self::Synth),
            _ => None,
        }
    }
}

impl std::fmt::Display for SoundPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = match self {
            Self::EightBit => "eight-bit",
            Self::Subtle => "subtle",
            Self::Synth => "synth",
            Self::System => "system",
            Self::None => "none",
        };
        f.write_str(id)
    }
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
    /// Per-event sound choice (default/synth/eight-bit/system/off/builtin:*)
    event_sound: Arc<Mutex<std::collections::HashMap<SoundEvent, String>>>,
    /// Custom sound id to file path
    custom_sounds: Arc<Mutex<std::collections::HashMap<String, String>>>,
    /// Active sound pack
    sound_pack: Arc<Mutex<SoundPack>>,
    /// Filter out sounds for probe/health-check sessions
    probe_filter: Arc<Mutex<bool>>,
    /// Suppress sounds during quiet hours
    quiet_hours: Arc<Mutex<QuietHours>>,
    /// Recent play timestamps for evolab-style spam suppression.
    recent_play_times: Arc<Mutex<Vec<Instant>>>,
}

#[derive(Debug, Clone)]
struct QuietHours {
    enabled: bool,
    start: String,
    end: String,
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
                event_sound: Arc::new(Mutex::new(std::collections::HashMap::new())),
                custom_sounds: Arc::new(Mutex::new(std::collections::HashMap::new())),
                sound_pack: Arc::new(Mutex::new(SoundPack::Synth)),
                probe_filter: Arc::new(Mutex::new(false)),
                quiet_hours: Arc::new(Mutex::new(QuietHours {
                    enabled: false,
                    start: "22:00".to_string(),
                    end: "08:00".to_string(),
                })),
                recent_play_times: Arc::new(Mutex::new(Vec::new())),
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

    pub fn set_event_rule(&self, event: SoundEvent, enabled: bool, sound: String) {
        self.set_event_enabled(event, enabled);
        if let Ok(mut map) = self.event_sound.lock() {
            map.insert(event, sound);
        }
    }

    pub fn set_custom_sounds(&self, sounds: Vec<(String, String)>) {
        if let Ok(mut map) = self.custom_sounds.lock() {
            map.clear();
            for (id, path) in sounds {
                map.insert(id, path);
            }
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

    /// Configure quiet hours. Invalid times are ignored at playback time.
    pub fn set_quiet_hours(&self, enabled: bool, start: String, end: String) {
        if let Ok(mut quiet_hours) = self.quiet_hours.lock() {
            quiet_hours.enabled = enabled;
            quiet_hours.start = start;
            quiet_hours.end = end;
        }
    }

    fn is_quiet_hours_active(&self) -> bool {
        let quiet_hours = match self.quiet_hours.lock() {
            Ok(config) => config.clone(),
            Err(_) => return false,
        };
        if !quiet_hours.enabled {
            return false;
        }
        let Some(start) = parse_minutes(&quiet_hours.start) else {
            return false;
        };
        let Some(end) = parse_minutes(&quiet_hours.end) else {
            return false;
        };
        let now = chrono::Local::now();
        let current = now.hour() * 60 + now.minute();
        if start <= end {
            current >= start && current < end
        } else {
            current >= start || current < end
        }
    }

    fn is_spamming(&self) -> bool {
        let Ok(mut recent) = self.recent_play_times.lock() else {
            return false;
        };
        let now = Instant::now();
        recent.retain(|timestamp| now.duration_since(*timestamp) < SPAM_WINDOW);
        if recent.len() >= SPAM_THRESHOLD {
            return true;
        }
        recent.push(now);
        false
    }

    /// Play a sound event (non-blocking, spawns on a new sink)
    pub fn play(&self, event: SoundEvent) {
        if !self.is_enabled() || !self.is_event_enabled(event) || self.is_quiet_hours_active() {
            return;
        }
        if self.is_spamming() {
            return;
        }

        let volume = self.volume.lock().map(|v| *v).unwrap_or(0.7);
        let default_pack = self
            .sound_pack
            .lock()
            .map(|p| *p)
            .unwrap_or(SoundPack::EightBit);
        let choice = self
            .event_sound
            .lock()
            .ok()
            .and_then(|map| map.get(&event).cloned())
            .unwrap_or_else(|| "default".to_string());
        if choice == "off" || choice == "none" {
            return;
        }
        let pack = sound_choice_pack(&choice).unwrap_or(default_pack);
        let builtin = builtin_sound_id(&choice);
        let custom_path = custom_sound_id(&choice).and_then(|id| {
            self.custom_sounds
                .lock()
                .ok()
                .and_then(|map| map.get(id).cloned())
        });

        let sink = match Sink::try_new(&self.stream_handle) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to create audio sink: {}", e);
                return;
            }
        };

        sink.set_volume(volume);

        if let Some(path) = custom_path {
            if !self.play_audio_file(&sink, &path) {
                self.play_synth(&sink, SoundEvent::TaskComplete);
            }
        } else if let Some(builtin) = builtin {
            self.play_builtin(&sink, builtin);
        } else {
            match pack {
                SoundPack::EightBit => self.play_eight_bit(&sink, event),
                SoundPack::Subtle => self.play_subtle(&sink, event),
                SoundPack::Synth => self.play_synth(&sink, event),
                SoundPack::System => self.play_system(&sink, event),
                SoundPack::None => return,
            }
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
            SoundEvent::SessionEnd => {
                sink.append(sine_wave(500.0, Duration::from_millis(70)));
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
            SoundEvent::PlanApproval => {
                sink.append(square_wave(523.0, Duration::from_millis(100)));
                sink.append(square_wave(659.0, Duration::from_millis(100)));
                sink.append(square_wave(784.0, Duration::from_millis(100)));
            }
            SoundEvent::ContextLimit => {
                sink.append(square_wave(300.0, Duration::from_millis(200)));
            }
            SoundEvent::Boot => {
                sink.append(square_wave(660.0, Duration::from_millis(80)));
                sink.append(square_wave(880.0, Duration::from_millis(90)));
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
            SoundEvent::SessionEnd => {
                sink.append(sine_wave(420.0, Duration::from_millis(60)));
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
            SoundEvent::PlanApproval => {
                sink.append(sine_wave(440.0, Duration::from_millis(70)));
                sink.append(sine_wave(554.0, Duration::from_millis(70)));
                sink.append(sine_wave(660.0, Duration::from_millis(70)));
            }
            SoundEvent::ContextLimit => {
                sink.append(sine_wave(280.0, Duration::from_millis(120)));
            }
            SoundEvent::Boot => {
                sink.append(sine_wave(520.0, Duration::from_millis(60)));
                sink.append(sine_wave(680.0, Duration::from_millis(70)));
            }
        }
    }

    /// Synth pack: brighter short tones matching evolab's default island feel.
    fn play_synth(&self, sink: &Sink, event: SoundEvent) {
        match event {
            SoundEvent::SessionStart => {
                sink.append(sine_wave(660.0, Duration::from_millis(70)));
                sink.append(sine_wave(990.0, Duration::from_millis(90)));
            }
            SoundEvent::SessionEnd => {
                sink.append(sine_wave(740.0, Duration::from_millis(60)));
                sink.append(sine_wave(494.0, Duration::from_millis(90)));
            }
            SoundEvent::TaskComplete => {
                sink.append(sine_wave(523.25, Duration::from_millis(80)));
                sink.append(sine_wave(659.25, Duration::from_millis(80)));
                sink.append(sine_wave(783.99, Duration::from_millis(110)));
            }
            SoundEvent::TaskError => {
                sink.append(sine_wave(392.0, Duration::from_millis(90)));
                sink.append(sine_wave(196.0, Duration::from_millis(140)));
            }
            SoundEvent::NeedsApproval => {
                sink.append(sine_wave(880.0, Duration::from_millis(80)));
                sink.append(silence(Duration::from_millis(45)));
                sink.append(sine_wave(880.0, Duration::from_millis(80)));
            }
            SoundEvent::TaskConfirmation => {
                sink.append(sine_wave(740.0, Duration::from_millis(60)));
                sink.append(sine_wave(932.0, Duration::from_millis(75)));
            }
            SoundEvent::PlanApproval => {
                sink.append(sine_wave(440.0, Duration::from_millis(120)));
                sink.append(sine_wave(554.0, Duration::from_millis(120)));
                sink.append(sine_wave(660.0, Duration::from_millis(120)));
            }
            SoundEvent::ContextLimit => {
                sink.append(sine_wave(330.0, Duration::from_millis(160)));
                sink.append(sine_wave(277.0, Duration::from_millis(160)));
            }
            SoundEvent::Boot => {
                sink.append(sine_wave(587.0, Duration::from_millis(55)));
                sink.append(sine_wave(740.0, Duration::from_millis(65)));
                sink.append(sine_wave(988.0, Duration::from_millis(80)));
            }
        }
    }

    /// System pack: restrained tones for users who want less prominent audio.
    fn play_system(&self, sink: &Sink, event: SoundEvent) {
        sink.set_volume(sink.volume() * 0.75);
        self.play_subtle(sink, event);
    }

    fn play_audio_file(&self, sink: &Sink, path: &str) -> bool {
        let Ok(file) = File::open(path) else {
            return false;
        };
        let Ok(source) = Decoder::new(BufReader::new(file)) else {
            return false;
        };
        sink.append(source);
        true
    }

    fn play_audio_bytes(&self, sink: &Sink, bytes: &'static [u8]) -> bool {
        let Ok(source) = Decoder::new(Cursor::new(bytes)) else {
            return false;
        };
        sink.append(source);
        true
    }

    fn play_builtin(&self, sink: &Sink, id: &str) {
        match id {
            "hey-bro" => {
                if !self.play_audio_bytes(sink, include_bytes!("assets/hey-bro.wav")) {
                    self.play_synth(sink, SoundEvent::Boot);
                }
            }
            "hero" => {
                sink.append(sine_wave(392.0, Duration::from_millis(105)));
                sink.append(sine_wave(523.0, Duration::from_millis(105)));
                sink.append(sine_wave(659.0, Duration::from_millis(105)));
            }
            "glass" => {
                sink.append(sine_wave(659.0, Duration::from_millis(125)));
                sink.append(sine_wave(880.0, Duration::from_millis(125)));
            }
            "ping" => {
                sink.append(sine_wave(988.0, Duration::from_millis(150)));
            }
            "pop" => {
                sink.append(sine_wave(620.0, Duration::from_millis(70)));
                sink.append(sine_wave(760.0, Duration::from_millis(70)));
            }
            "submarine" => {
                sink.append(sine_wave(330.0, Duration::from_millis(115)));
                sink.append(sine_wave(294.0, Duration::from_millis(115)));
                sink.append(sine_wave(262.0, Duration::from_millis(115)));
            }
            "basso" => {
                sink.append(sine_wave(196.0, Duration::from_millis(155)));
                sink.append(sine_wave(147.0, Duration::from_millis(155)));
            }
            "sosumi" => {
                sink.append(sine_wave(698.0, Duration::from_millis(90)));
                sink.append(sine_wave(523.0, Duration::from_millis(90)));
                sink.append(sine_wave(392.0, Duration::from_millis(90)));
            }
            "bottle" => {
                sink.append(sine_wave(740.0, Duration::from_millis(75)));
                sink.append(sine_wave(880.0, Duration::from_millis(75)));
            }
            "tink" => {
                sink.append(sine_wave(1046.0, Duration::from_millis(60)));
                sink.append(sine_wave(1318.0, Duration::from_millis(60)));
            }
            "morse" => {
                sink.append(square_wave(880.0, Duration::from_millis(45)));
                sink.append(square_wave(880.0, Duration::from_millis(45)));
                sink.append(square_wave(880.0, Duration::from_millis(45)));
            }
            "funk" => {
                sink.append(square_wave(247.0, Duration::from_millis(85)));
                sink.append(square_wave(370.0, Duration::from_millis(85)));
                sink.append(square_wave(494.0, Duration::from_millis(85)));
            }
            "purr" => {
                sink.append(sine_wave(220.0, Duration::from_millis(120)));
                sink.append(sine_wave(247.0, Duration::from_millis(120)));
                sink.append(sine_wave(220.0, Duration::from_millis(120)));
            }
            "blow" => {
                sink.append(sine_wave(320.0, Duration::from_millis(140)));
                sink.append(sine_wave(260.0, Duration::from_millis(140)));
            }
            "frog" => {
                sink.append(square_wave(175.0, Duration::from_millis(120)));
                sink.append(square_wave(210.0, Duration::from_millis(120)));
            }
            _ => self.play_synth(sink, SoundEvent::TaskComplete),
        }
    }
}

// ── Waveform generators ──────────────────────────────────────────

fn sound_choice_pack(choice: &str) -> Option<SoundPack> {
    if choice == "default" || choice.starts_with("builtin:") || choice.starts_with("custom:") {
        return None;
    }
    SoundPack::from_id(choice)
}

fn builtin_sound_id(choice: &str) -> Option<&str> {
    choice.strip_prefix("builtin:")
}

fn custom_sound_id(choice: &str) -> Option<&str> {
    choice.strip_prefix("custom:")
}

fn parse_minutes(value: &str) -> Option<u32> {
    let (hours, minutes) = value.split_once(':')?;
    let hours: u32 = hours.parse().ok()?;
    let minutes: u32 = minutes.parse().ok()?;
    if hours < 24 && minutes < 60 {
        Some(hours * 60 + minutes)
    } else {
        None
    }
}

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
