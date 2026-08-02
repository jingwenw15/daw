//! Real-time audio engine primitives.

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample,
};
use std::{f64::consts::TAU, fmt, fs, io, path::Path};
use std::{
    sync::{
        atomic::{AtomicU32, AtomicUsize, Ordering},
        Arc,
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
    let device_name = device
        .name()
        .unwrap_or_else(|_| "default output".to_owned());
    let supported_config = device
        .default_output_config()
        .map_err(|error| PlaybackError::DefaultConfig(error.to_string()))?;
    let sample_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels();
    let buffer = Arc::new(render_sine(
        440.0,
        duration_seconds,
        0.20,
        sample_rate,
        channels,
    )?);
    let next_sample = Arc::new(AtomicUsize::new(0));
    let stream_errors = Arc::new(AtomicU32::new(0));
    let stream_config = supported_config.config();

    let stream = match supported_config.sample_format() {
        cpal::SampleFormat::F32 => build_test_tone_stream::<f32>(
            &device,
            &stream_config,
            Arc::clone(&buffer),
            Arc::clone(&next_sample),
            &stream_errors,
        ),
        cpal::SampleFormat::I16 => build_test_tone_stream::<i16>(
            &device,
            &stream_config,
            Arc::clone(&buffer),
            Arc::clone(&next_sample),
            &stream_errors,
        ),
        cpal::SampleFormat::U16 => build_test_tone_stream::<u16>(
            &device,
            &stream_config,
            Arc::clone(&buffer),
            Arc::clone(&next_sample),
            &stream_errors,
        ),
        format => Err(PlaybackError::BuildStream(format!(
            "unsupported sample format: {format:?}"
        ))),
    }?;

    stream
        .play()
        .map_err(|error| PlaybackError::PlayStream(error.to_string()))?;
    thread::sleep(Duration::from_secs_f32(duration_seconds + 0.10));
    drop(stream);

    Ok(PlaybackReport {
        device_name,
        sample_rate,
        channels,
        frames_played: next_sample.load(Ordering::Relaxed) / usize::from(channels),
        stream_errors: stream_errors.load(Ordering::Relaxed),
    })
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

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::InvalidSettings(message) => formatter.write_str(message),
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

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidSettings(_) => None,
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
    stream_errors: &Arc<AtomicU32>,
) -> Result<cpal::Stream, PlaybackError>
where
    T: Sample + FromSample<f32> + cpal::SizedSample,
{
    let error_counter = Arc::clone(stream_errors);
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| write_playback_data(output, &buffer, &next_sample),
            move |_| {
                error_counter.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|error| PlaybackError::BuildStream(error.to_string()))
}

fn write_playback_data<T>(output: &mut [T], buffer: &AudioBuffer, next_sample: &AtomicUsize)
where
    T: Sample + FromSample<f32>,
{
    for sample in output {
        let index = next_sample.fetch_add(1, Ordering::Relaxed);
        let value = buffer.samples.get(index).copied().unwrap_or(0.0);
        *sample = T::from_sample(value);
    }
}

#[cfg(test)]
mod tests {
    use super::{render_silence, render_sine, write_wav, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE};
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

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("daw-engine-{}-{name}", std::process::id()))
    }
}
