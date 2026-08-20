//! Real-time audio engine primitives.

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample,
};
use std::{f64::consts::TAU, fmt, fs, io, path::Path};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

/// Crate version exposed for smoke tests and diagnostics.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default offline render sample rate.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// Default offline render channel count.
pub const DEFAULT_CHANNELS: u16 = 2;

/// Interleaved floating-point audio buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
    /// Interleaved samples in `[-1.0, 1.0]`.
    pub samples: Vec<f32>,
}

/// Error returned by offline rendering.
#[derive(Debug)]
pub enum RenderError {
    /// Filesystem failure.
    Io(io::Error),
    /// Render settings are invalid.
    InvalidSettings(String),
    /// WAV file is unsupported or malformed.
    UnsupportedWav(String),
}

/// Summary returned by a real-time playback smoke run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackReport {
    /// Output device name.
    pub device_name: String,
    /// Stream sample rate.
    pub sample_rate: u32,
    /// Stream channel count.
    pub channels: u16,
    /// Frames rendered by the callback.
    pub frames_played: usize,
    /// Stream error callback count.
    pub stream_errors: u32,
}

/// Summary returned with a captured input buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingReport {
    /// Input device name.
    pub device_name: String,
    /// Stream sample rate.
    pub sample_rate: u32,
    /// Stream channel count.
    pub channels: u16,
    /// Frames captured into the buffer.
    pub frames_recorded: usize,
    /// Stream error callback count.
    pub stream_errors: u32,
}

/// Captured input audio and device metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedAudio {
    /// Recorded samples.
    pub buffer: AudioBuffer,
    /// Capture report.
    pub report: RecordingReport,
}

/// Handle for an active real-time playback stream.
pub struct PlaybackTransport {
    stream: Option<cpal::Stream>,
    device_name: String,
    sample_rate: u32,
    channels: u16,
    next_sample: Arc<AtomicUsize>,
    stream_errors: Arc<AtomicU32>,
    stop_requested: Arc<AtomicBool>,
    total_samples: Option<usize>,
}

/// Handle for an active real-time recording stream.
pub struct RecordingTransport {
    stream: Option<cpal::Stream>,
    device_name: String,
    sample_rate: u32,
    channels: u16,
    samples: Arc<Mutex<Vec<f32>>>,
    samples_written: Arc<AtomicUsize>,
    stream_errors: Arc<AtomicU32>,
    stop_requested: Arc<AtomicBool>,
    target_samples: Option<usize>,
}

/// Error returned by real-time playback.
#[derive(Debug)]
pub enum PlaybackError {
    /// No default output device is available.
    NoOutputDevice,
    /// Backend failed to provide a default stream config.
    DefaultConfig(String),
    /// Backend failed to build the stream.
    BuildStream(String),
    /// Backend failed to start the stream.
    PlayStream(String),
    /// Render settings are invalid.
    Render(RenderError),
}

/// Error returned by real-time recording.
#[derive(Debug)]
pub enum RecordingError {
    /// No default input device is available.
    NoInputDevice,
    /// Backend failed to provide a default stream config.
    DefaultConfig(String),
    /// Backend failed to build the stream.
    BuildStream(String),
    /// Backend failed to start the stream.
    PlayStream(String),
    /// Render settings are invalid.
    Render(RenderError),
    /// Internal capture buffer could not be read.
    BufferUnavailable,
}

/// Render a deterministic sine test tone.
///
/// # Errors
///
/// Returns an error if render settings are invalid.
pub fn render_sine(
    frequency_hz: f32,
    duration_seconds: f32,
    gain: f32,
    sample_rate: u32,
    channels: u16,
) -> Result<AudioBuffer, RenderError> {
    validate_render_settings(duration_seconds, sample_rate, channels)?;
    if frequency_hz <= 0.0 {
        return Err(RenderError::InvalidSettings(
            "frequency must be greater than zero".to_owned(),
        ));
    }

    let frames = frames_for_duration(duration_seconds, sample_rate);
    let channels_usize = usize::from(channels);
    let mut samples = Vec::with_capacity(frames * channels_usize);

    fill_sine_samples(
        &mut samples,
        frames,
        channels,
        sample_rate,
        frequency_hz,
        gain,
    );

    Ok(AudioBuffer {
        sample_rate,
        channels,
        samples,
    })
}

/// Render a synthesized metronome click track.
///
/// The first beat of each bar is accented. Clicks are generated from short
/// decaying sine bursts, so no bundled samples or copyrighted assets are used.
///
/// # Errors
///
/// Returns an error if tempo, meter, duration, or render settings are invalid.
pub fn render_metronome(
    tempo_bpm: u16,
    beats_per_bar: u16,
    bars: u32,
    sample_rate: u32,
    channels: u16,
) -> Result<AudioBuffer, RenderError> {
    validate_render_settings(1.0, sample_rate, channels)?;
    if tempo_bpm == 0 {
        return Err(RenderError::InvalidSettings(
            "tempo must be greater than zero".to_owned(),
        ));
    }
    if beats_per_bar == 0 {
        return Err(RenderError::InvalidSettings(
            "beats per bar must be greater than zero".to_owned(),
        ));
    }
    if bars == 0 {
        return Err(RenderError::InvalidSettings(
            "bar count must be greater than zero".to_owned(),
        ));
    }

    let beat_frames = samples_per_beat(tempo_bpm, sample_rate);
    let beat_count = usize::from(beats_per_bar)
        .checked_mul(
            usize::try_from(bars)
                .map_err(|_| RenderError::InvalidSettings("bar count is too large".to_owned()))?,
        )
        .ok_or_else(|| RenderError::InvalidSettings("metronome is too long".to_owned()))?;
    let frames = beat_frames
        .checked_mul(beat_count)
        .ok_or_else(|| RenderError::InvalidSettings("metronome is too long".to_owned()))?;
    let mut buffer = AudioBuffer {
        sample_rate,
        channels,
        samples: vec![0.0; frames * usize::from(channels)],
    };

    for beat in 0..beat_count {
        let frame = beat * beat_frames;
        let accented = beat % usize::from(beats_per_bar) == 0;
        let (frequency, gain) = if accented {
            (1_760.0_f32, 0.70_f32)
        } else {
            (1_100.0_f32, 0.42_f32)
        };
        mix_decaying_sine_click(&mut buffer, frame, frequency, gain, 0.045);
    }

    Ok(buffer)
}

/// Play a short sine test tone through the default output device.
///
/// # Errors
///
/// Returns an error if no output device exists, the stream cannot be built, or
/// the stream cannot be started.
pub fn play_test_tone(duration_seconds: f32) -> Result<PlaybackReport, PlaybackError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(PlaybackError::NoOutputDevice)?;
    let supported_config = device
        .default_output_config()
        .map_err(|error| PlaybackError::DefaultConfig(error.to_string()))?;
    let sample_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels();
    let buffer = render_sine(440.0, duration_seconds, 0.20, sample_rate, channels)?;
    play_buffer(buffer, duration_seconds + 0.10)
}

/// Play a rendered buffer through the default output device.
///
/// # Errors
///
/// Returns an error if no output device exists, the stream cannot be built, or
/// the stream cannot be started.
pub fn play_buffer(
    buffer: AudioBuffer,
    hold_seconds: f32,
) -> Result<PlaybackReport, PlaybackError> {
    let mut transport = start_buffer_playback(buffer)?;
    thread::sleep(Duration::from_secs_f32(hold_seconds));
    Ok(transport.stop())
}

/// Start playing a rendered buffer through the default output device.
///
/// The returned transport owns the native audio stream. Dropping or stopping it
/// ends playback.
///
/// # Errors
///
/// Returns an error if no output device exists, the stream cannot be built, or
/// the stream cannot be started.
pub fn start_buffer_playback(buffer: AudioBuffer) -> Result<PlaybackTransport, PlaybackError> {
    start_buffer_playback_inner(buffer, false)
}

/// Start looping a rendered buffer through the default output device.
///
/// The returned transport owns the native audio stream. Dropping or stopping it
/// ends playback.
///
/// # Errors
///
/// Returns an error if no output device exists, the stream cannot be built, or
/// the stream cannot be started.
pub fn start_looping_buffer_playback(
    buffer: AudioBuffer,
) -> Result<PlaybackTransport, PlaybackError> {
    start_buffer_playback_inner(buffer, true)
}

fn start_buffer_playback_inner(
    buffer: AudioBuffer,
    looping: bool,
) -> Result<PlaybackTransport, PlaybackError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(PlaybackError::NoOutputDevice)?;
    let device_name = device
        .name()
        .unwrap_or_else(|_| "default output".to_owned());
    let supported_config = device
        .default_output_config()
        .map_err(|error| PlaybackError::DefaultConfig(error.to_string()))?;
    let sample_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels();
    let total_samples = buffer.samples.len();
    let buffer = Arc::new(buffer);
    let next_sample = Arc::new(AtomicUsize::new(0));
    let stream_errors = Arc::new(AtomicU32::new(0));
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stream_config = supported_config.config();

    let stream = match supported_config.sample_format() {
        cpal::SampleFormat::F32 => build_test_tone_stream::<f32>(
            &device,
            &stream_config,
            Arc::clone(&buffer),
            Arc::clone(&next_sample),
            Arc::clone(&stop_requested),
            looping,
            &stream_errors,
        ),
        cpal::SampleFormat::I16 => build_test_tone_stream::<i16>(
            &device,
            &stream_config,
            Arc::clone(&buffer),
            Arc::clone(&next_sample),
            Arc::clone(&stop_requested),
            looping,
            &stream_errors,
        ),
        cpal::SampleFormat::U16 => build_test_tone_stream::<u16>(
            &device,
            &stream_config,
            Arc::clone(&buffer),
            Arc::clone(&next_sample),
            Arc::clone(&stop_requested),
            looping,
            &stream_errors,
        ),
        format => Err(PlaybackError::BuildStream(format!(
            "unsupported sample format: {format:?}"
        ))),
    }?;

    stream
        .play()
        .map_err(|error| PlaybackError::PlayStream(error.to_string()))?;

    Ok(PlaybackTransport {
        stream: Some(stream),
        device_name,
        sample_rate,
        channels,
        next_sample,
        stream_errors,
        stop_requested,
        total_samples: (!looping).then_some(total_samples),
    })
}

/// Record a fixed-duration snippet from the default input device.
///
/// # Errors
///
/// Returns an error if no input device exists, the input stream cannot be built
/// or started, or the duration is invalid.
pub fn record_input(duration_seconds: f32) -> Result<RecordedAudio, RecordingError> {
    let mut transport = start_limited_input_recording(duration_seconds)?;
    while !transport.is_finished() {
        thread::sleep(Duration::from_millis(10));
    }
    transport.stop()
}

/// Start recording from the default input device.
///
/// The returned transport owns the native input stream. Calling `stop` finalizes
/// the captured buffer.
///
/// # Errors
///
/// Returns an error if no input device exists, the input stream cannot be built
/// or started.
pub fn start_input_recording() -> Result<RecordingTransport, RecordingError> {
    start_input_recording_with_limit(None)
}

fn start_limited_input_recording(
    duration_seconds: f32,
) -> Result<RecordingTransport, RecordingError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(RecordingError::NoInputDevice)?;
    let device_name = device.name().unwrap_or_else(|_| "default input".to_owned());
    let supported_config = device
        .default_input_config()
        .map_err(|error| RecordingError::DefaultConfig(error.to_string()))?;
    let sample_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels();
    validate_render_settings(duration_seconds, sample_rate, channels)?;
    let target_samples = frames_for_duration(duration_seconds, sample_rate) * usize::from(channels);
    start_input_recording_from_config(
        &device,
        device_name,
        &supported_config,
        Some(target_samples),
    )
}

fn start_input_recording_with_limit(
    target_samples: Option<usize>,
) -> Result<RecordingTransport, RecordingError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(RecordingError::NoInputDevice)?;
    let device_name = device.name().unwrap_or_else(|_| "default input".to_owned());
    let supported_config = device
        .default_input_config()
        .map_err(|error| RecordingError::DefaultConfig(error.to_string()))?;
    start_input_recording_from_config(&device, device_name, &supported_config, target_samples)
}

fn start_input_recording_from_config(
    device: &cpal::Device,
    device_name: String,
    supported_config: &cpal::SupportedStreamConfig,
    target_samples: Option<usize>,
) -> Result<RecordingTransport, RecordingError> {
    let sample_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels();
    let samples = Arc::new(Mutex::new(Vec::<f32>::with_capacity(
        target_samples.unwrap_or(0),
    )));
    let samples_written = Arc::new(AtomicUsize::new(0));
    let stream_errors = Arc::new(AtomicU32::new(0));
    let stop_requested = Arc::new(AtomicBool::new(false));
    let stream_config = supported_config.config();

    let stream = match supported_config.sample_format() {
        cpal::SampleFormat::F32 => build_recording_stream::<f32>(
            device,
            &stream_config,
            Arc::clone(&samples),
            Arc::clone(&samples_written),
            Arc::clone(&stop_requested),
            target_samples,
            &stream_errors,
        ),
        cpal::SampleFormat::I16 => build_recording_stream::<i16>(
            device,
            &stream_config,
            Arc::clone(&samples),
            Arc::clone(&samples_written),
            Arc::clone(&stop_requested),
            target_samples,
            &stream_errors,
        ),
        cpal::SampleFormat::U16 => build_recording_stream::<u16>(
            device,
            &stream_config,
            Arc::clone(&samples),
            Arc::clone(&samples_written),
            Arc::clone(&stop_requested),
            target_samples,
            &stream_errors,
        ),
        format => Err(RecordingError::BuildStream(format!(
            "unsupported sample format: {format:?}"
        ))),
    }?;

    stream
        .play()
        .map_err(|error| RecordingError::PlayStream(error.to_string()))?;

    Ok(RecordingTransport {
        stream: Some(stream),
        device_name,
        sample_rate,
        channels,
        samples,
        samples_written,
        stream_errors,
        stop_requested,
        target_samples,
    })
}

/// Read a 16-bit PCM WAV file into an audio buffer.
///
/// # Errors
///
/// Returns an error if the file cannot be read or is not supported PCM WAV.
pub fn read_wav(path: &Path) -> Result<AudioBuffer, RenderError> {
    let bytes = fs::read(path)?;
    parse_pcm16_wav(&bytes)
}

/// Mix a clip buffer into a destination at a start frame.
pub fn mix_clip(
    destination: &mut AudioBuffer,
    clip: &AudioBuffer,
    start_frame: usize,
    volume_percent: u16,
    muted: bool,
) {
    if muted || destination.channels != clip.channels || destination.sample_rate != clip.sample_rate
    {
        return;
    }

    let destination_channels = usize::from(destination.channels);
    let gain = f32::from(volume_percent) / 100.0;
    for clip_frame in 0..clip.frames() {
        let destination_frame = start_frame + clip_frame;
        if destination_frame >= destination.frames() {
            break;
        }
        for channel in 0..destination_channels {
            let destination_index = destination_frame * destination_channels + channel;
            let clip_index = clip_frame * destination_channels + channel;
            destination.samples[destination_index] = (destination.samples[destination_index]
                + clip.samples[clip_index] * gain)
                .clamp(-1.0, 1.0);
        }
    }
}

/// Apply non-destructive linear clip fade ramps to an audio buffer segment.
pub fn apply_clip_fades(
    buffer: &mut AudioBuffer,
    clip_offset_frames: u64,
    clip_duration_frames: u64,
    fade_in_frames: u64,
    fade_out_frames: u64,
) {
    if buffer.samples.is_empty() || buffer.channels == 0 {
        return;
    }
    let channels = usize::from(buffer.channels);
    for frame in 0..buffer.frames() {
        let Ok(frame_u64) = u64::try_from(frame) else {
            break;
        };
        let clip_position = clip_offset_frames.saturating_add(frame_u64);
        let mut gain = 1.0_f32;
        if fade_in_frames > 0 && clip_position < fade_in_frames {
            gain = gain.min(fade_ratio(clip_position, fade_in_frames));
        }
        if fade_out_frames > 0 && clip_position < clip_duration_frames {
            let remaining = clip_duration_frames - clip_position;
            if remaining < fade_out_frames {
                gain = gain.min(fade_ratio(remaining, fade_out_frames));
            }
        }
        if gain >= 1.0 {
            continue;
        }
        for channel in 0..channels {
            buffer.samples[frame * channels + channel] *= gain;
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn fade_ratio(numerator: u64, denominator: u64) -> f32 {
    numerator as f32 / denominator as f32
}

/// Convert an audio buffer to a different channel count.
#[must_use]
pub fn convert_channels(buffer: &AudioBuffer, channels: u16) -> AudioBuffer {
    if buffer.channels == channels {
        return buffer.clone();
    }

    let source_channels = usize::from(buffer.channels);
    let destination_channels = usize::from(channels);
    let mut samples = Vec::with_capacity(buffer.frames() * destination_channels);
    for frame in 0..buffer.frames() {
        for channel in 0..destination_channels {
            let source_channel = channel.min(source_channels.saturating_sub(1));
            let sample = buffer.samples[frame * source_channels + source_channel];
            samples.push(sample);
        }
    }

    AudioBuffer {
        sample_rate: buffer.sample_rate,
        channels,
        samples,
    }
}

/// Return a frame-range slice of an audio buffer.
#[must_use]
pub fn slice_frames(buffer: &AudioBuffer, start_frame: usize, frame_count: usize) -> AudioBuffer {
    let channels = usize::from(buffer.channels);
    let start_sample = start_frame
        .saturating_mul(channels)
        .min(buffer.samples.len());
    let end_sample = start_sample
        .saturating_add(frame_count.saturating_mul(channels))
        .min(buffer.samples.len());
    AudioBuffer {
        sample_rate: buffer.sample_rate,
        channels: buffer.channels,
        samples: buffer.samples[start_sample..end_sample].to_vec(),
    }
}

/// Render silence for a minimal project render placeholder.
///
/// # Errors
///
/// Returns an error if render settings are invalid.
pub fn render_silence(
    duration_seconds: f32,
    sample_rate: u32,
    channels: u16,
) -> Result<AudioBuffer, RenderError> {
    validate_render_settings(duration_seconds, sample_rate, channels)?;
    let frames = frames_for_duration(duration_seconds, sample_rate);
    Ok(AudioBuffer {
        sample_rate,
        channels,
        samples: vec![0.0; frames * usize::from(channels)],
    })
}

/// Write an interleaved buffer to a 16-bit PCM WAV file.
///
/// # Errors
///
/// Returns an error if the output cannot be written or the buffer shape is
/// invalid.
pub fn write_wav(path: &Path, buffer: &AudioBuffer) -> Result<(), RenderError> {
    let channel_count = usize::from(buffer.channels);
    if buffer.channels == 0 || !buffer.samples.len().is_multiple_of(channel_count) {
        return Err(RenderError::InvalidSettings(
            "buffer sample count must be divisible by channel count".to_owned(),
        ));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let bytes_per_sample = 2_u16;
    let bits_per_sample = bytes_per_sample * 8;
    let data_bytes = u32::try_from(buffer.samples.len() * usize::from(bytes_per_sample))
        .map_err(|_| RenderError::InvalidSettings("render too large for WAV".to_owned()))?;
    let riff_size = 36_u32
        .checked_add(data_bytes)
        .ok_or_else(|| RenderError::InvalidSettings("render too large for WAV".to_owned()))?;
    let byte_rate = buffer
        .sample_rate
        .checked_mul(u32::from(buffer.channels))
        .and_then(|value| value.checked_mul(u32::from(bytes_per_sample)))
        .ok_or_else(|| RenderError::InvalidSettings("invalid WAV byte rate".to_owned()))?;
    let block_align = buffer
        .channels
        .checked_mul(bytes_per_sample)
        .ok_or_else(|| RenderError::InvalidSettings("invalid WAV block align".to_owned()))?;

    let mut bytes = Vec::with_capacity(44 + usize::try_from(data_bytes).unwrap_or(0));
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&riff_size.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&buffer.channels.to_le_bytes());
    bytes.extend_from_slice(&buffer.sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());

    for sample in &buffer.samples {
        bytes.extend_from_slice(&sample_to_i16(*sample).to_le_bytes());
    }

    fs::write(path, bytes)?;
    Ok(())
}

impl AudioBuffer {
    /// Number of sample frames in this buffer.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }
}

impl PlaybackTransport {
    /// Stop playback and return a final report.
    #[must_use]
    pub fn stop(&mut self) -> PlaybackReport {
        self.stop_requested.store(true, Ordering::Relaxed);
        drop(self.stream.take());
        self.report()
    }

    /// Return the latest playback counters without stopping the stream.
    #[must_use]
    pub fn report(&self) -> PlaybackReport {
        PlaybackReport {
            device_name: self.device_name.clone(),
            sample_rate: self.sample_rate,
            channels: self.channels,
            frames_played: self.next_sample.load(Ordering::Relaxed) / usize::from(self.channels),
            stream_errors: self.stream_errors.load(Ordering::Relaxed),
        }
    }

    /// Return true once the stream has consumed the complete source buffer.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.total_samples
            .is_some_and(|total_samples| self.next_sample.load(Ordering::Relaxed) >= total_samples)
    }
}

impl Drop for PlaybackTransport {
    fn drop(&mut self) {
        self.stop_requested.store(true, Ordering::Relaxed);
    }
}

impl RecordingTransport {
    /// Return the currently captured audio without stopping the input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal capture buffer cannot be read.
    pub fn snapshot(&self) -> Result<RecordedAudio, RecordingError> {
        let samples = self
            .samples
            .lock()
            .map_err(|_| RecordingError::BufferUnavailable)?
            .clone();
        let buffer = AudioBuffer {
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples,
        };
        let report = RecordingReport {
            device_name: self.device_name.clone(),
            sample_rate: self.sample_rate,
            channels: self.channels,
            frames_recorded: buffer.frames(),
            stream_errors: self.stream_errors.load(Ordering::Relaxed),
        };
        Ok(RecordedAudio { buffer, report })
    }

    /// Stop recording and return the captured audio.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal capture buffer cannot be finalized.
    pub fn stop(&mut self) -> Result<RecordedAudio, RecordingError> {
        self.stop_requested.store(true, Ordering::Relaxed);
        drop(self.stream.take());
        self.snapshot()
    }

    /// Return the latest recording counters without stopping the stream.
    #[must_use]
    pub fn report(&self) -> RecordingReport {
        RecordingReport {
            device_name: self.device_name.clone(),
            sample_rate: self.sample_rate,
            channels: self.channels,
            frames_recorded: self.samples_written.load(Ordering::Relaxed)
                / usize::from(self.channels),
            stream_errors: self.stream_errors.load(Ordering::Relaxed),
        }
    }

    /// Return true once a fixed-duration recording has reached its limit.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.target_samples
            .is_some_and(|target| self.samples_written.load(Ordering::Relaxed) >= target)
    }
}

impl Drop for RecordingTransport {
    fn drop(&mut self) {
        self.stop_requested.store(true, Ordering::Relaxed);
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::InvalidSettings(message) | Self::UnsupportedWav(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl fmt::Display for PlaybackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutputDevice => formatter.write_str("no default output device available"),
            Self::DefaultConfig(message)
            | Self::BuildStream(message)
            | Self::PlayStream(message) => formatter.write_str(message),
            Self::Render(error) => write!(formatter, "{error}"),
        }
    }
}

impl fmt::Display for RecordingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInputDevice => formatter.write_str("no default input device available"),
            Self::DefaultConfig(message)
            | Self::BuildStream(message)
            | Self::PlayStream(message) => formatter.write_str(message),
            Self::Render(error) => write!(formatter, "{error}"),
            Self::BufferUnavailable => formatter.write_str("recording buffer is unavailable"),
        }
    }
}

impl std::error::Error for PlaybackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render(error) => Some(error),
            Self::NoOutputDevice
            | Self::DefaultConfig(_)
            | Self::BuildStream(_)
            | Self::PlayStream(_) => None,
        }
    }
}

impl From<RenderError> for PlaybackError {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}

impl From<RenderError> for RecordingError {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}

impl std::error::Error for RecordingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render(error) => Some(error),
            Self::NoInputDevice
            | Self::DefaultConfig(_)
            | Self::BuildStream(_)
            | Self::PlayStream(_)
            | Self::BufferUnavailable => None,
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidSettings(_) | Self::UnsupportedWav(_) => None,
        }
    }
}

impl From<io::Error> for RenderError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn validate_render_settings(
    duration_seconds: f32,
    sample_rate: u32,
    channels: u16,
) -> Result<(), RenderError> {
    if duration_seconds <= 0.0 {
        return Err(RenderError::InvalidSettings(
            "duration must be greater than zero".to_owned(),
        ));
    }
    if sample_rate == 0 {
        return Err(RenderError::InvalidSettings(
            "sample rate must be greater than zero".to_owned(),
        ));
    }
    if channels == 0 {
        return Err(RenderError::InvalidSettings(
            "channel count must be greater than zero".to_owned(),
        ));
    }
    Ok(())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn frames_for_duration(duration_seconds: f32, sample_rate: u32) -> usize {
    (f64::from(duration_seconds) * f64::from(sample_rate)).round() as usize
}

#[allow(clippy::cast_precision_loss)]
fn fill_sine_samples(
    samples: &mut Vec<f32>,
    frames: usize,
    channels: u16,
    sample_rate: u32,
    frequency_hz: f32,
    gain: f32,
) {
    for frame in 0..frames {
        let time = frame as f64 / f64::from(sample_rate);
        let sample = (time * f64::from(frequency_hz) * TAU).sin() * f64::from(gain);
        for _ in 0..channels {
            samples.push(sample_to_f32(sample.clamp(-1.0, 1.0)));
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn samples_per_beat(tempo_bpm: u16, sample_rate: u32) -> usize {
    ((f64::from(sample_rate) * 60.0) / f64::from(tempo_bpm.max(1))).round() as usize
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn mix_decaying_sine_click(
    buffer: &mut AudioBuffer,
    start_frame: usize,
    frequency_hz: f32,
    gain: f32,
    duration_seconds: f32,
) {
    let channels = usize::from(buffer.channels);
    let click_frames = frames_for_duration(duration_seconds, buffer.sample_rate);
    for offset in 0..click_frames {
        let frame = start_frame + offset;
        if frame >= buffer.frames() {
            break;
        }
        let time = offset as f64 / f64::from(buffer.sample_rate);
        let decay = (-time * 90.0).exp();
        let sample = (time * f64::from(frequency_hz) * TAU).sin() * f64::from(gain) * decay;
        for channel in 0..channels {
            let index = frame * channels + channel;
            buffer.samples[index] =
                (buffer.samples[index] + sample_to_f32(sample)).clamp(-1.0, 1.0);
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn sample_to_f32(sample: f64) -> f32 {
    sample as f32
}

#[allow(clippy::cast_possible_truncation)]
fn sample_to_i16(sample: f32) -> i16 {
    let pcm = f64::from(sample.clamp(-1.0, 1.0)) * f64::from(i16::MAX);
    pcm.round() as i16
}

fn build_test_tone_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<AudioBuffer>,
    next_sample: Arc<AtomicUsize>,
    stop_requested: Arc<AtomicBool>,
    looping: bool,
    stream_errors: &Arc<AtomicU32>,
) -> Result<cpal::Stream, PlaybackError>
where
    T: Sample + FromSample<f32> + cpal::SizedSample,
{
    let error_counter = Arc::clone(stream_errors);
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                write_playback_data(output, &buffer, &next_sample, &stop_requested, looping);
            },
            move |_| {
                error_counter.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|error| PlaybackError::BuildStream(error.to_string()))
}

fn build_recording_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    samples_written: Arc<AtomicUsize>,
    stop_requested: Arc<AtomicBool>,
    target_samples: Option<usize>,
    stream_errors: &Arc<AtomicU32>,
) -> Result<cpal::Stream, RecordingError>
where
    T: Sample + cpal::SizedSample,
    f32: FromSample<T>,
{
    let error_counter = Arc::clone(stream_errors);
    device
        .build_input_stream(
            config,
            move |input: &[T], _| {
                write_recording_data(
                    input,
                    &samples,
                    &samples_written,
                    &stop_requested,
                    target_samples,
                );
            },
            move |_| {
                error_counter.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|error| RecordingError::BuildStream(error.to_string()))
}

fn write_recording_data<T>(
    input: &[T],
    samples: &Mutex<Vec<f32>>,
    samples_written: &AtomicUsize,
    stop_requested: &AtomicBool,
    target_samples: Option<usize>,
) where
    T: Sample,
    f32: FromSample<T>,
{
    if stop_requested.load(Ordering::Relaxed)
        || target_samples.is_some_and(|target| samples_written.load(Ordering::Relaxed) >= target)
    {
        return;
    }
    let Ok(mut samples) = samples.lock() else {
        return;
    };
    for sample in input {
        if target_samples.is_some_and(|target| samples.len() >= target) {
            break;
        }
        samples.push(f32::from_sample(*sample));
    }
    samples_written.store(samples.len(), Ordering::Relaxed);
}

fn write_playback_data<T>(
    output: &mut [T],
    buffer: &AudioBuffer,
    next_sample: &AtomicUsize,
    stop_requested: &AtomicBool,
    looping: bool,
) where
    T: Sample + FromSample<f32>,
{
    for sample in output {
        let mut index = if stop_requested.load(Ordering::Relaxed) {
            buffer.samples.len()
        } else {
            next_sample.fetch_add(1, Ordering::Relaxed)
        };
        if looping && !buffer.samples.is_empty() {
            index %= buffer.samples.len();
        }
        let value = buffer.samples.get(index).copied().unwrap_or(0.0);
        *sample = T::from_sample(value);
    }
}

fn parse_pcm16_wav(bytes: &[u8]) -> Result<AudioBuffer, RenderError> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(RenderError::UnsupportedWav(
            "expected RIFF/WAVE file".to_owned(),
        ));
    }

    let mut cursor = 12;
    let mut channels = None;
    let mut sample_rate = None;
    let mut bits_per_sample = None;
    let mut audio_format = None;
    let mut data = None;

    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes([
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]);
        cursor += 8;
        let chunk_size = usize::try_from(size)
            .map_err(|_| RenderError::UnsupportedWav("WAV chunk is too large".to_owned()))?;
        if cursor + chunk_size > bytes.len() {
            return Err(RenderError::UnsupportedWav(
                "WAV chunk extends past end of file".to_owned(),
            ));
        }
        match id {
            b"fmt " => {
                if chunk_size < 16 {
                    return Err(RenderError::UnsupportedWav(
                        "WAV fmt chunk is too short".to_owned(),
                    ));
                }
                audio_format = Some(u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]));
                channels = Some(u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]));
                sample_rate = Some(u32::from_le_bytes([
                    bytes[cursor + 4],
                    bytes[cursor + 5],
                    bytes[cursor + 6],
                    bytes[cursor + 7],
                ]));
                bits_per_sample =
                    Some(u16::from_le_bytes([bytes[cursor + 14], bytes[cursor + 15]]));
            }
            b"data" => {
                data = Some(&bytes[cursor..cursor + chunk_size]);
            }
            _ => {}
        }
        cursor += chunk_size + (chunk_size % 2);
    }

    if audio_format != Some(1) || bits_per_sample != Some(16) {
        return Err(RenderError::UnsupportedWav(
            "only 16-bit PCM WAV files are supported".to_owned(),
        ));
    }

    let channels = channels.ok_or_else(|| {
        RenderError::UnsupportedWav("WAV file is missing channel count".to_owned())
    })?;
    let sample_rate = sample_rate
        .ok_or_else(|| RenderError::UnsupportedWav("WAV file is missing sample rate".to_owned()))?;
    let data =
        data.ok_or_else(|| RenderError::UnsupportedWav("WAV file is missing data".to_owned()))?;
    if data.len() % 2 != 0 {
        return Err(RenderError::UnsupportedWav(
            "WAV data length must be even for PCM16".to_owned(),
        ));
    }

    let mut samples = Vec::with_capacity(data.len() / 2);
    for chunk in data.chunks_exact(2) {
        let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
        samples.push(f32::from(sample) / f32::from(i16::MAX));
    }

    Ok(AudioBuffer {
        sample_rate,
        channels,
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        apply_clip_fades, convert_channels, mix_clip, read_wav, render_metronome, render_silence,
        render_sine, slice_frames, write_wav, AudioBuffer, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn exposes_package_version() {
        assert!(!super::VERSION.is_empty());
    }

    #[test]
    fn renders_deterministic_sine_buffer() {
        let first = render_sine(440.0, 1.0, 0.25, DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS)
            .expect("render sine");
        let second = render_sine(440.0, 1.0, 0.25, DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS)
            .expect("render sine");

        assert_eq!(first, second);
        assert_eq!(first.frames(), 48_000);
        assert_eq!(first.samples.len(), 96_000);
        assert!(first.samples[0].abs() < f32::EPSILON);
    }

    #[test]
    fn renders_silence() {
        let buffer =
            render_silence(0.5, DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS).expect("render silence");

        assert_eq!(buffer.frames(), 24_000);
        assert!(buffer.samples.iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn renders_metronome_with_bar_accents() {
        let buffer =
            render_metronome(120, 4, 2, DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS).expect("metronome");

        assert_eq!(buffer.frames(), 192_000);
        assert!(buffer.samples.iter().any(|sample| sample.abs() > 0.0));
        let accent_peak = peak_between(&buffer, 0, 2_000);
        let regular_peak = peak_between(&buffer, 24_000, 26_000);
        assert!(accent_peak > regular_peak);
    }

    #[test]
    fn writes_valid_wav_header() {
        let output = temp_file("tone.wav");
        let buffer = render_sine(440.0, 0.01, 0.25, DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS)
            .expect("render sine");

        write_wav(&output, &buffer).expect("write wav");
        let bytes = fs::read(&output).expect("read wav");

        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        fs::remove_file(output).expect("cleanup");
    }

    #[test]
    fn reads_written_wav_and_mixes_clip() {
        let output = temp_file("read-mix.wav");
        let clip = render_sine(440.0, 0.01, 0.25, DEFAULT_SAMPLE_RATE, DEFAULT_CHANNELS)
            .expect("render sine");
        write_wav(&output, &clip).expect("write wav");

        let decoded = read_wav(&output).expect("read wav");
        let mut destination = AudioBuffer {
            sample_rate: DEFAULT_SAMPLE_RATE,
            channels: DEFAULT_CHANNELS,
            samples: vec![0.0; decoded.samples.len() + 100],
        };
        mix_clip(&mut destination, &decoded, 10, 100, false);

        assert_eq!(decoded.frames(), clip.frames());
        assert!(destination.samples.iter().any(|sample| sample.abs() > 0.0));
        fs::remove_file(output).expect("cleanup");
    }

    #[test]
    fn applies_clip_fades_to_buffer_frames() {
        let mut buffer = AudioBuffer {
            sample_rate: DEFAULT_SAMPLE_RATE,
            channels: 1,
            samples: vec![1.0; 6],
        };

        apply_clip_fades(&mut buffer, 0, 6, 3, 3);

        assert!(buffer.samples[0].abs() < f32::EPSILON);
        assert!((buffer.samples[1] - (1.0 / 3.0)).abs() < f32::EPSILON);
        assert!((buffer.samples[2] - (2.0 / 3.0)).abs() < f32::EPSILON);
        assert!((buffer.samples[3] - 1.0).abs() < f32::EPSILON);
        assert!((buffer.samples[4] - (2.0 / 3.0)).abs() < f32::EPSILON);
        assert!((buffer.samples[5] - (1.0 / 3.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn converts_mono_to_stereo() {
        let mono = AudioBuffer {
            sample_rate: DEFAULT_SAMPLE_RATE,
            channels: 1,
            samples: vec![0.25, -0.25],
        };

        let stereo = convert_channels(&mono, 2);

        assert_eq!(stereo.channels, 2);
        assert_eq!(stereo.samples, vec![0.25, 0.25, -0.25, -0.25]);
    }

    #[test]
    fn slices_frame_ranges() {
        let buffer = AudioBuffer {
            sample_rate: DEFAULT_SAMPLE_RATE,
            channels: 2,
            samples: vec![0.0, 0.1, 1.0, 1.1, 2.0, 2.1],
        };

        let sliced = slice_frames(&buffer, 1, 2);

        assert_eq!(sliced.channels, 2);
        assert_eq!(sliced.samples, vec![1.0, 1.1, 2.0, 2.1]);
    }

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("daw-engine-{}-{name}", std::process::id()))
    }

    fn peak_between(buffer: &AudioBuffer, start_frame: usize, end_frame: usize) -> f32 {
        let channels = usize::from(buffer.channels);
        let start = start_frame * channels;
        let end = (end_frame * channels).min(buffer.samples.len());
        buffer.samples[start..end]
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0, f32::max)
    }
}
