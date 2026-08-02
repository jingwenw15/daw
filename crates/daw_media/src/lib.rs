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

/// Error returned by media operations.
#[derive(Debug)]
pub enum MediaError {
    /// Filesystem failure.
    Io(io::Error),
    /// JSON serialization or parsing failure.
    Json(serde_json::Error),
    /// Media object could not be found.
    Missing(String),
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

    let object = MediaObject {
        hash,
        original_path: source_path.to_string_lossy().into_owned(),
        extension,
        byte_size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sample_rate: None,
        channels: None,
        duration_samples: None,
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

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Missing(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for MediaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Missing(_) => None,
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

#[cfg(test)]
mod tests {
    use super::{
        import_media, list_media, media_index_path, media_objects_path, relink_media, verify_media,
        VERSION,
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
}
