//! Media import, indexing, and storage primitives.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt::{self, Write as _},
    fs, io,
    path::{Path, PathBuf},
};

/// Crate version exposed for smoke tests and diagnostics.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Media index path under a project directory.
pub const MEDIA_INDEX_FILE_NAME: &str = "index.json";

/// Default number of peak buckets in a waveform preview.
pub const DEFAULT_WAVEFORM_POINTS: usize = 512;

/// Content-addressed media object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MediaObject {
    /// SHA-256 content hash.
    pub hash: String,
    /// Original import path for user-facing relinking context.
    pub original_path: String,
    /// File extension, lowercased without the dot.
    pub extension: Option<String>,
    /// File byte size.
    pub byte_size: u64,
    /// Sample rate when known.
    pub sample_rate: Option<u32>,
    /// Channel count when known.
    pub channels: Option<u16>,
    /// Duration in samples when known.
    pub duration_samples: Option<u64>,
}

/// Project media index.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MediaIndex {
    /// Imported media objects.
    pub objects: Vec<MediaObject>,
}

/// Verification status for one media object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaVerification {
    /// Media object hash.
    pub hash: String,
    /// True when the object exists and its content matches the hash.
    pub ok: bool,
    /// Human-readable verification detail.
    pub message: String,
}

/// One waveform preview peak bucket.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaveformPeak {
    /// Minimum normalized sample value in this bucket.
    pub min: f32,
    /// Maximum normalized sample value in this bucket.
    pub max: f32,
}

/// Disposable waveform preview data for a media object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaveformSummary {
    /// Media object hash.
    pub hash: String,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count in the source media.
    pub channels: u16,
    /// Source sample frames represented by each peak bucket.
    pub frames_per_peak: u64,
    /// Peak buckets across the media object.
    pub peaks: Vec<WaveformPeak>,
}

/// Error returned by media operations.
#[derive(Debug)]
pub enum MediaError {
    /// Filesystem failure.
    Io(io::Error),
    /// JSON serialization or parsing failure.
    Json(serde_json::Error),
    /// Media object could not be found.
    Missing(String),
    /// Media object cannot be decoded for this operation.
    Unsupported(String),
}

/// Return the media index path for a project.
#[must_use]
pub fn media_index_path(project_dir: &Path) -> PathBuf {
    project_dir.join("media").join(MEDIA_INDEX_FILE_NAME)
}

/// Return the media object root path for a project.
#[must_use]
pub fn media_objects_path(project_dir: &Path) -> PathBuf {
    project_dir.join("media").join("objects")
}

/// Return the disposable waveform cache path for a project.
#[must_use]
pub fn waveform_cache_path(project_dir: &Path) -> PathBuf {
    project_dir.join("cache").join("waveforms")
}

/// Import a media file into content-addressed storage.
///
/// # Errors
///
/// Returns an error if the source cannot be read, copied, or indexed.
pub fn import_media(project_dir: &Path, source_path: &Path) -> Result<MediaObject, MediaError> {
    fs::create_dir_all(media_objects_path(project_dir))?;
    fs::create_dir_all(waveform_cache_path(project_dir))?;

    let bytes = fs::read(source_path)?;
    let hash = sha256_hex(&bytes);
    let extension = normalized_extension(source_path);
    let object_path = media_object_path(project_dir, &hash, extension.as_deref());

    if let Some(parent) = object_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if !object_path.exists() {
        fs::write(&object_path, &bytes)?;
    }

    let metadata = parse_pcm16_wav(&bytes).ok().map(|wav| {
        (
            wav.sample_rate,
            wav.channels,
            u64::try_from(wav.frames()).unwrap_or(u64::MAX),
        )
    });
    let object = MediaObject {
        hash,
        original_path: source_path.to_string_lossy().into_owned(),
        extension,
        byte_size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sample_rate: metadata.map(|metadata| metadata.0),
        channels: metadata.map(|metadata| metadata.1),
        duration_samples: metadata.map(|metadata| metadata.2),
    };
    upsert_media_object(project_dir, object.clone())?;
    Ok(object)
}

/// List indexed media objects.
///
/// # Errors
///
/// Returns an error if the media index cannot be read or parsed.
pub fn list_media(project_dir: &Path) -> Result<Vec<MediaObject>, MediaError> {
    Ok(load_index(project_dir)?.objects)
}

/// Verify all indexed media objects against their content hashes.
///
/// # Errors
///
/// Returns an error if the media index cannot be read or an object cannot be read.
pub fn verify_media(project_dir: &Path) -> Result<Vec<MediaVerification>, MediaError> {
    let index = load_index(project_dir)?;
    let mut results = Vec::with_capacity(index.objects.len());

    for object in index.objects {
        let path = media_object_path(project_dir, &object.hash, object.extension.as_deref());
        if !path.exists() {
            results.push(MediaVerification {
                hash: object.hash,
                ok: false,
                message: "missing object file".to_owned(),
            });
            continue;
        }

        let bytes = fs::read(path)?;
        let actual = sha256_hex(&bytes);
        let ok = actual == object.hash;
        let message = if ok {
            "ok".to_owned()
        } else {
            format!("hash mismatch: expected {}, got {actual}", object.hash)
        };
        results.push(MediaVerification {
            hash: object.hash,
            ok,
            message,
        });
    }

    Ok(results)
}

/// Relink a missing media object to a replacement source file.
///
/// # Errors
///
/// Returns an error if the hash is unknown or the replacement cannot be copied.
pub fn relink_media(
    project_dir: &Path,
    hash: &str,
    replacement_path: &Path,
) -> Result<MediaObject, MediaError> {
    let mut index = load_index(project_dir)?;
    let position = index
        .objects
        .iter()
        .position(|object| object.hash == hash)
        .ok_or_else(|| MediaError::Missing(format!("unknown media hash: {hash}")))?;

    let bytes = fs::read(replacement_path)?;
    let replacement_hash = sha256_hex(&bytes);
    if replacement_hash != hash {
        return Err(MediaError::Missing(format!(
            "replacement content hash {replacement_hash} does not match {hash}"
        )));
    }

    let extension = normalized_extension(replacement_path);
    let path = media_object_path(project_dir, hash, extension.as_deref());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;

    index.objects[position].original_path = replacement_path.to_string_lossy().into_owned();
    index.objects[position].extension = extension;
    let object = index.objects[position].clone();
    save_index(project_dir, &index)?;
    Ok(object)
}

/// Return the waveform cache path for a media hash.
#[must_use]
pub fn waveform_summary_path(project_dir: &Path, hash: &str) -> PathBuf {
    waveform_cache_path(project_dir).join(format!("{hash}.json"))
}

/// Generate and cache waveform preview data for one imported media object.
///
/// # Errors
///
/// Returns an error if the media object is unknown, missing, unsupported, or
/// the waveform cache cannot be written.
pub fn generate_waveform(
    project_dir: &Path,
    hash: &str,
    target_points: usize,
) -> Result<WaveformSummary, MediaError> {
    let object = load_index(project_dir)?
        .objects
        .into_iter()
        .find(|object| object.hash == hash)
        .ok_or_else(|| MediaError::Missing(format!("unknown media hash: {hash}")))?;
    let path = media_object_path(project_dir, &object.hash, object.extension.as_deref());
    let bytes = fs::read(path)?;
    let wav = parse_pcm16_wav(&bytes)?;
    let summary = waveform_from_pcm16(hash, &wav, target_points.max(1));
    save_waveform(project_dir, &summary)?;
    Ok(summary)
}

/// Generate waveform previews for every imported media object that can be decoded.
///
/// # Errors
///
/// Returns an error if media index or cache access fails.
pub fn generate_waveforms(
    project_dir: &Path,
    target_points: usize,
) -> Result<Vec<WaveformSummary>, MediaError> {
    let objects = list_media(project_dir)?;
    let mut waveforms = Vec::new();
    for object in objects {
        if let Ok(waveform) = generate_waveform(project_dir, &object.hash, target_points) {
            waveforms.push(waveform);
        }
    }
    Ok(waveforms)
}

/// Load cached waveform preview data for one media hash.
///
/// # Errors
///
/// Returns an error if the cache file cannot be read or parsed.
pub fn load_waveform(project_dir: &Path, hash: &str) -> Result<WaveformSummary, MediaError> {
    Ok(serde_json::from_str(&fs::read_to_string(
        waveform_summary_path(project_dir, hash),
    )?)?)
}

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Missing(message) | Self::Unsupported(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for MediaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Missing(_) | Self::Unsupported(_) => None,
        }
    }
}

impl From<io::Error> for MediaError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for MediaError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

fn upsert_media_object(project_dir: &Path, object: MediaObject) -> Result<(), MediaError> {
    let mut index = load_index(project_dir)?;
    if let Some(existing) = index
        .objects
        .iter_mut()
        .find(|existing| existing.hash == object.hash)
    {
        *existing = object;
    } else {
        index.objects.push(object);
        index
            .objects
            .sort_by(|left, right| left.hash.cmp(&right.hash));
    }
    save_index(project_dir, &index)
}

fn load_index(project_dir: &Path) -> Result<MediaIndex, MediaError> {
    let path = media_index_path(project_dir);
    if !path.exists() {
        return Ok(MediaIndex::default());
    }

    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn save_index(project_dir: &Path, index: &MediaIndex) -> Result<(), MediaError> {
    fs::create_dir_all(project_dir.join("media"))?;
    let mut json = serde_json::to_string_pretty(index)?;
    json.push('\n');
    fs::write(media_index_path(project_dir), json)?;
    Ok(())
}

fn save_waveform(project_dir: &Path, waveform: &WaveformSummary) -> Result<(), MediaError> {
    fs::create_dir_all(waveform_cache_path(project_dir))?;
    let mut json = serde_json::to_string_pretty(waveform)?;
    json.push('\n');
    fs::write(waveform_summary_path(project_dir, &waveform.hash), json)?;
    Ok(())
}

/// Return the content-addressed path for a media object.
#[must_use]
pub fn media_object_path(project_dir: &Path, hash: &str, extension: Option<&str>) -> PathBuf {
    let prefix = hash.get(0..2).unwrap_or("xx");
    let file_name = extension.map_or_else(
        || hash.to_owned(),
        |extension| format!("{hash}.{extension}"),
    );
    media_objects_path(project_dir)
        .join("sha256")
        .join(prefix)
        .join(file_name)
}

fn normalized_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .map(str::to_ascii_lowercase)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Pcm16Wav {
    sample_rate: u32,
    channels: u16,
    samples: Vec<i16>,
}

impl Pcm16Wav {
    fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }
}

fn parse_pcm16_wav(bytes: &[u8]) -> Result<Pcm16Wav, MediaError> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(MediaError::Unsupported(
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
            .map_err(|_| MediaError::Unsupported("WAV chunk is too large".to_owned()))?;
        if cursor + chunk_size > bytes.len() {
            return Err(MediaError::Unsupported(
                "WAV chunk extends past end of file".to_owned(),
            ));
        }
        match id {
            b"fmt " => {
                if chunk_size < 16 {
                    return Err(MediaError::Unsupported(
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
            b"data" => data = Some(&bytes[cursor..cursor + chunk_size]),
            _ => {}
        }
        cursor += chunk_size + (chunk_size % 2);
    }

    if audio_format != Some(1) || bits_per_sample != Some(16) {
        return Err(MediaError::Unsupported(
            "only 16-bit PCM WAV files are supported".to_owned(),
        ));
    }
    let channels = channels
        .ok_or_else(|| MediaError::Unsupported("WAV file is missing channel count".to_owned()))?;
    if channels == 0 {
        return Err(MediaError::Unsupported(
            "WAV channel count must be greater than zero".to_owned(),
        ));
    }
    let sample_rate = sample_rate
        .ok_or_else(|| MediaError::Unsupported("WAV file is missing sample rate".to_owned()))?;
    let data =
        data.ok_or_else(|| MediaError::Unsupported("WAV file is missing data".to_owned()))?;
    if data.len() % 2 != 0 {
        return Err(MediaError::Unsupported(
            "WAV data length must be even for PCM16".to_owned(),
        ));
    }

    let mut samples = Vec::with_capacity(data.len() / 2);
    for chunk in data.chunks_exact(2) {
        samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
    }
    if samples.len() % usize::from(channels) != 0 {
        return Err(MediaError::Unsupported(
            "WAV data length must align with channel count".to_owned(),
        ));
    }

    Ok(Pcm16Wav {
        sample_rate,
        channels,
        samples,
    })
}

fn waveform_from_pcm16(hash: &str, wav: &Pcm16Wav, target_points: usize) -> WaveformSummary {
    let frames = wav.frames();
    let frames_per_peak = frames.div_ceil(target_points).max(1);
    let mut peaks = Vec::new();
    let channels = usize::from(wav.channels);

    for start_frame in (0..frames).step_by(frames_per_peak) {
        let end_frame = (start_frame + frames_per_peak).min(frames);
        let mut min = 1.0_f32;
        let mut max = -1.0_f32;
        for frame in start_frame..end_frame {
            for channel in 0..channels {
                let sample =
                    f32::from(wav.samples[frame * channels + channel]) / f32::from(i16::MAX);
                min = min.min(sample);
                max = max.max(sample);
            }
        }
        peaks.push(WaveformPeak { min, max });
    }

    WaveformSummary {
        hash: hash.to_owned(),
        sample_rate: wav.sample_rate,
        channels: wav.channels,
        frames_per_peak: u64::try_from(frames_per_peak).unwrap_or(u64::MAX),
        peaks,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        generate_waveform, import_media, list_media, load_waveform, media_index_path,
        media_objects_path, relink_media, verify_media, waveform_summary_path, VERSION,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    #[test]
    fn exposes_package_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn imports_and_indexes_media() {
        let project_dir = temp_project_dir("import");
        let source = project_dir.join("source.wav");
        fs::create_dir_all(&project_dir).expect("create temp dir");
        fs::write(&source, b"fake wav bytes").expect("write source");

        let object = import_media(&project_dir, &source).expect("import media");
        let objects = list_media(&project_dir).expect("list media");
        let verification = verify_media(&project_dir).expect("verify media");

        assert_eq!(objects, vec![object.clone()]);
        assert_eq!(object.extension.as_deref(), Some("wav"));
        assert!(media_index_path(&project_dir).is_file());
        assert!(media_objects_path(&project_dir).join("sha256").is_dir());
        assert_eq!(verification.len(), 1);
        assert!(verification[0].ok);

        fs::remove_dir_all(project_dir).expect("cleanup");
    }

    #[test]
    fn relinks_missing_media_with_matching_hash() {
        let project_dir = temp_project_dir("relink");
        let source = project_dir.join("source.aiff");
        let replacement = project_dir.join("replacement.aiff");
        fs::create_dir_all(&project_dir).expect("create temp dir");
        fs::write(&source, b"same bytes").expect("write source");
        fs::write(&replacement, b"same bytes").expect("write replacement");

        let object = import_media(&project_dir, &source).expect("import media");
        let object_file = find_object_file(&project_dir, &object.hash);
        fs::remove_file(object_file).expect("remove object");
        let missing = verify_media(&project_dir).expect("verify missing");
        let relinked = relink_media(&project_dir, &object.hash, &replacement).expect("relink");
        let verified = verify_media(&project_dir).expect("verify relinked");

        assert!(!missing[0].ok);
        assert_eq!(relinked.original_path, replacement.to_string_lossy());
        assert!(verified[0].ok);

        fs::remove_dir_all(project_dir).expect("cleanup");
    }

    #[test]
    fn generates_and_loads_waveform_cache() {
        let project_dir = temp_project_dir("waveform");
        let source = project_dir.join("source.wav");
        fs::create_dir_all(&project_dir).expect("create temp dir");
        fs::write(&source, pcm16_wav_bytes(&[0, i16::MAX, i16::MIN, 0])).expect("write source");

        let object = import_media(&project_dir, &source).expect("import media");
        let waveform = generate_waveform(&project_dir, &object.hash, 2).expect("waveform");
        let loaded = load_waveform(&project_dir, &object.hash).expect("load waveform");

        assert_eq!(object.sample_rate, Some(48_000));
        assert_eq!(object.channels, Some(1));
        assert_eq!(object.duration_samples, Some(4));
        assert_eq!(waveform.peaks.len(), 2);
        assert!(waveform_summary_path(&project_dir, &object.hash).is_file());
        assert_eq!(loaded, waveform);

        fs::remove_dir_all(project_dir).expect("cleanup");
    }

    fn temp_project_dir(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("daw-media-{test_name}-{}", std::process::id()))
    }

    fn find_object_file(project_dir: &Path, hash: &str) -> PathBuf {
        let prefix = hash.get(0..2).expect("hash prefix");
        fs::read_dir(media_objects_path(project_dir).join("sha256").join(prefix))
            .expect("read object dir")
            .next()
            .expect("object entry")
            .expect("object entry")
            .path()
    }

    fn pcm16_wav_bytes(samples: &[i16]) -> Vec<u8> {
        let data_bytes = u32::try_from(samples.len() * 2).expect("data size");
        let riff_size = 36_u32 + data_bytes;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&riff_size.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&48_000_u32.to_le_bytes());
        bytes.extend_from_slice(&96_000_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_bytes.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }
}
