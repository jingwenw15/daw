//! Project data model primitives.

use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// Crate version exposed for smoke tests and diagnostics.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Current on-disk project schema version.
pub const PROJECT_SCHEMA_VERSION: u32 = 1;

/// Canonical project manifest file name.
pub const PROJECT_FILE_NAME: &str = "project.daw.json";

/// Append-only command log location beneath `history`.
pub const COMMAND_LOG_FILE_NAME: &str = "commands.jsonl";

/// Snapshot directory location beneath `history`.
pub const SNAPSHOT_DIR_NAME: &str = "snapshots";

/// Branch directory location beneath `history`.
pub const BRANCH_DIR_NAME: &str = "branches";

/// Active branch pointer location beneath `history`.
pub const ACTIVE_BRANCH_FILE_NAME: &str = "active_branch.txt";

/// Default branch name for new projects.
pub const DEFAULT_BRANCH_NAME: &str = "main";

static NEXT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// User-editable project document persisted to `project.daw.json`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Project {
    /// Schema version used by migration code.
    pub schema_version: u32,
    /// Stable project identifier.
    pub id: StableId,
    /// Human-readable project name.
    pub name: String,
    /// Track list in timeline order.
    pub tracks: Vec<Track>,
    /// Media references known to the project.
    pub media: Vec<MediaReference>,
}

/// A timeline track.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Track {
    /// Stable track identifier.
    pub id: StableId,
    /// Human-readable track name.
    pub name: String,
    /// Linear volume percentage, where 100 is unity gain.
    #[serde(default = "default_track_volume_percent")]
    pub volume_percent: u16,
    /// True when this track should be silent.
    #[serde(default)]
    pub muted: bool,
    /// True when this track is soloed.
    #[serde(default)]
    pub solo: bool,
    /// Clips placed on this track.
    pub clips: Vec<Clip>,
}

/// A clip placement on the timeline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Clip {
    /// Stable clip identifier.
    pub id: StableId,
    /// Stable media identifier referenced by this clip.
    pub media_id: StableId,
    /// Timeline start in samples.
    pub start_sample: u64,
    /// Clip duration in samples.
    pub duration_samples: u64,
}

/// A media object referenced by the project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MediaReference {
    /// Stable media identifier.
    pub id: StableId,
    /// Content hash, once imported into the media store.
    pub content_hash: Option<String>,
    /// Original import path, retained for relinking.
    pub original_path: Option<String>,
}

/// Stable text identifier used in serialized project documents.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct StableId(String);

/// A user edit that can be replayed to rebuild project state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectCommand {
    /// Add a new timeline track.
    AddTrack {
        /// Track to append.
        track: Track,
    },
    /// Remove a timeline track and its clips.
    RemoveTrack {
        /// Track to remove.
        track_id: StableId,
    },
    /// Add or update a media reference.
    AddMediaReference {
        /// Media reference to store.
        media: MediaReference,
    },
    /// Add a clip to an existing track.
    AddClip {
        /// Track receiving the clip.
        track_id: StableId,
        /// Clip to append.
        clip: Clip,
    },
    /// Move or resize an existing clip.
    SetClipPlacement {
        /// Clip receiving the placement settings.
        clip_id: StableId,
        /// Timeline start in samples.
        start_sample: u64,
        /// Clip duration in samples.
        duration_samples: u64,
    },
    /// Remove an existing clip from its track.
    RemoveClip {
        /// Clip to remove.
        clip_id: StableId,
    },
    /// Update mixer controls for an existing track.
    SetTrackControls {
        /// Track receiving the mixer settings.
        track_id: StableId,
        /// Linear volume percentage.
        volume_percent: u16,
        /// True when this track should be silent.
        muted: bool,
        /// True when this track should be heard while non-soloed tracks are silent.
        solo: bool,
    },
    /// Replace current state with a stored snapshot.
    CheckoutSnapshot {
        /// Snapshot identifier to restore.
        snapshot_id: StableId,
    },
}

/// Command log entry with stable metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandEntry {
    /// Stable command identifier.
    pub id: StableId,
    /// Unix timestamp in milliseconds.
    pub timestamp_millis: u128,
    /// Command payload.
    pub command: ProjectCommand,
}

/// Full project snapshot metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Snapshot {
    /// Stable snapshot identifier.
    pub id: StableId,
    /// Unix timestamp in milliseconds.
    pub timestamp_millis: u128,
    /// Human-readable snapshot message.
    pub message: String,
    /// Full project state captured by this snapshot.
    pub project: Project,
}

/// Lightweight history item returned for CLI/UI presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryItem {
    /// Stable item identifier.
    pub id: StableId,
    /// Human-readable summary.
    pub summary: String,
}

/// Local project branch/take metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Branch {
    /// Human-readable branch name.
    pub name: String,
    /// Full project state stored for this branch.
    pub project: Project,
}

/// Human-readable diff between two project states.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectDiff {
    /// Tracks present only on the right side.
    pub added_tracks: Vec<String>,
    /// Tracks present only on the left side.
    pub removed_tracks: Vec<String>,
    /// Tracks with matching IDs but different content.
    pub changed_tracks: Vec<String>,
}

/// Result of a branch merge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeReport {
    /// Name of the merged source branch.
    pub source_branch: String,
    /// Track names added to the current branch.
    pub added_tracks: Vec<String>,
    /// Conflicts that blocked the merge.
    pub conflicts: Vec<String>,
}

/// Project validation failure.
#[derive(Debug, Eq, PartialEq)]
pub struct ValidationError {
    message: String,
}

/// Error returned by project file operations.
#[derive(Debug)]
pub enum ProjectIoError {
    /// Filesystem failure.
    Io(io::Error),
    /// JSON serialization or parsing failure.
    Json(serde_json::Error),
    /// Project document failed validation.
    Invalid(Vec<ValidationError>),
    /// Merge cannot proceed without user conflict resolution.
    Conflict(Vec<String>),
}

impl Project {
    /// Create a new empty project with a generated stable ID.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            id: StableId::new(),
            name: name.into(),
            tracks: Vec::new(),
            media: Vec::new(),
        }
    }

    /// Return all validation errors for this project.
    #[must_use]
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        if self.schema_version != PROJECT_SCHEMA_VERSION {
            errors.push(ValidationError::new(format!(
                "unsupported schema_version {}; expected {PROJECT_SCHEMA_VERSION}",
                self.schema_version
            )));
        }

        if self.name.trim().is_empty() {
            errors.push(ValidationError::new("project name must not be empty"));
        }

        let mut track_ids = Vec::with_capacity(self.tracks.len());
        for track in &self.tracks {
            if track.name.trim().is_empty() {
                errors.push(ValidationError::new(format!(
                    "track {} name must not be empty",
                    track.id
                )));
            }
            if track.volume_percent > 200 {
                errors.push(ValidationError::new(format!(
                    "track {} volume_percent must be between 0 and 200",
                    track.id
                )));
            }
            for clip in &track.clips {
                if clip.duration_samples == 0 {
                    errors.push(ValidationError::new(format!(
                        "clip {} duration_samples must be greater than zero",
                        clip.id
                    )));
                }
                if !self.media.iter().any(|media| media.id == clip.media_id) {
                    errors.push(ValidationError::new(format!(
                        "clip {} references unknown media {}",
                        clip.id, clip.media_id
                    )));
                }
            }
            track_ids.push(track.id.clone());
        }
        track_ids.sort();
        track_ids.dedup();
        if track_ids.len() != self.tracks.len() {
            errors.push(ValidationError::new("track IDs must be unique"));
        }

        let mut media_ids = Vec::with_capacity(self.media.len());
        for media in &self.media {
            media_ids.push(media.id.clone());
        }
        media_ids.sort();
        media_ids.dedup();
        if media_ids.len() != self.media.len() {
            errors.push(ValidationError::new("media IDs must be unique"));
        }

        errors
    }

    /// Serialize the project document using deterministic pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the project cannot be serialized to JSON.
    pub fn to_canonical_json(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        Ok(json)
    }
}

impl Track {
    /// Create a new empty track with a generated stable ID.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: StableId::new(),
            name: name.into(),
            volume_percent: default_track_volume_percent(),
            muted: false,
            solo: false,
            clips: Vec::new(),
        }
    }
}

impl StableId {
    /// Create a new stable identifier.
    #[must_use]
    pub fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let counter = u128::from(NEXT_ID_COUNTER.fetch_add(1, Ordering::Relaxed));
        let process = u128::from(std::process::id());
        let seed = nanos ^ (process << 64) ^ counter;
        let hex = format!("{seed:032x}");

        Self(format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        ))
    }

    /// Construct an ID from an existing serialized string.
    #[must_use]
    pub fn from_string(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Return the serialized string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ProjectCommand {
    fn summary(&self) -> String {
        match self {
            Self::AddTrack { track } => format!("add track '{}'", track.name),
            Self::RemoveTrack { track_id } => format!("remove track {track_id}"),
            Self::AddMediaReference { media } => {
                format!(
                    "add media {}",
                    media.content_hash.as_deref().unwrap_or("unknown")
                )
            }
            Self::AddClip { track_id, clip } => {
                format!("add clip {} to track {track_id}", clip.id)
            }
            Self::SetClipPlacement {
                clip_id,
                start_sample,
                duration_samples,
            } => {
                format!(
                    "set clip {clip_id} placement start={start_sample} duration={duration_samples}"
                )
            }
            Self::RemoveClip { clip_id } => format!("remove clip {clip_id}"),
            Self::SetTrackControls {
                track_id,
                volume_percent,
                muted,
                solo,
            } => {
                format!(
                    "set track {track_id} controls volume={volume_percent} muted={muted} solo={solo}"
                )
            }
            Self::CheckoutSnapshot { snapshot_id } => {
                format!("checkout snapshot {snapshot_id}")
            }
        }
    }
}

impl CommandEntry {
    fn new(command: ProjectCommand) -> Self {
        Self {
            id: StableId::new(),
            timestamp_millis: unix_timestamp_millis(),
            command,
        }
    }
}

impl Snapshot {
    fn new(message: impl Into<String>, project: Project) -> Self {
        Self {
            id: StableId::new(),
            timestamp_millis: unix_timestamp_millis(),
            message: message.into(),
            project,
        }
    }
}

impl Default for StableId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for StableId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl ValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Human-readable validation message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

impl fmt::Display for ProjectIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Invalid(errors) => {
                write!(formatter, "project validation failed")?;
                for error in errors {
                    write!(formatter, "\n- {error}")?;
                }
                Ok(())
            }
            Self::Conflict(conflicts) => {
                write!(formatter, "merge conflicts")?;
                for conflict in conflicts {
                    write!(formatter, "\n- {conflict}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ProjectIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Invalid(_) | Self::Conflict(_) => None,
        }
    }
}

impl From<io::Error> for ProjectIoError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ProjectIoError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Return the canonical project manifest path for a project directory.
#[must_use]
pub fn project_file_path(project_dir: &Path) -> PathBuf {
    project_dir.join(PROJECT_FILE_NAME)
}

/// Return the command log path for a project directory.
#[must_use]
pub fn command_log_path(project_dir: &Path) -> PathBuf {
    project_dir.join("history").join(COMMAND_LOG_FILE_NAME)
}

/// Return the base replay state path for a project directory.
#[must_use]
pub fn base_project_path(project_dir: &Path) -> PathBuf {
    project_dir.join("history").join("base.daw.json")
}

/// Return the snapshot directory path for a project directory.
#[must_use]
pub fn snapshot_dir_path(project_dir: &Path) -> PathBuf {
    project_dir.join("history").join(SNAPSHOT_DIR_NAME)
}

/// Return the branch directory path for a project directory.
#[must_use]
pub fn branch_dir_path(project_dir: &Path) -> PathBuf {
    project_dir.join("history").join(BRANCH_DIR_NAME)
}

/// Return the active branch pointer path.
#[must_use]
pub fn active_branch_path(project_dir: &Path) -> PathBuf {
    project_dir.join("history").join(ACTIVE_BRANCH_FILE_NAME)
}

/// Return the path for a stored snapshot.
#[must_use]
pub fn snapshot_path(project_dir: &Path, snapshot_id: &StableId) -> PathBuf {
    snapshot_dir_path(project_dir).join(format!("{snapshot_id}.json"))
}

/// Return the path for a stored branch.
#[must_use]
pub fn branch_path(project_dir: &Path, branch_name: &str) -> PathBuf {
    branch_dir_path(project_dir).join(format!("{branch_name}.json"))
}

/// Initialize an empty project directory and write its manifest.
///
/// # Errors
///
/// Returns an error if directories cannot be created, the project cannot be
/// serialized, or the generated project fails validation.
pub fn init_project(project_dir: &Path, name: &str) -> Result<Project, ProjectIoError> {
    fs::create_dir_all(project_dir)?;
    fs::create_dir_all(project_dir.join("history"))?;
    fs::create_dir_all(snapshot_dir_path(project_dir))?;
    fs::create_dir_all(branch_dir_path(project_dir))?;
    fs::create_dir_all(project_dir.join("media").join("objects"))?;
    fs::create_dir_all(project_dir.join("cache"))?;
    fs::create_dir_all(project_dir.join("exports"))?;

    let project = Project::new(name);
    save_project(project_dir, &project)?;
    fs::write(base_project_path(project_dir), project.to_canonical_json()?)?;
    fs::write(command_log_path(project_dir), "")?;
    fs::write(active_branch_path(project_dir), DEFAULT_BRANCH_NAME)?;
    save_branch(project_dir, DEFAULT_BRANCH_NAME, &project)?;
    Ok(project)
}

/// Load and validate a project manifest.
///
/// # Errors
///
/// Returns an error if the manifest cannot be read, parsed, or validated.
pub fn load_project(project_dir: &Path) -> Result<Project, ProjectIoError> {
    let json = fs::read_to_string(project_file_path(project_dir))?;
    let project = serde_json::from_str::<Project>(&json)?;
    let errors = project.validate();
    if errors.is_empty() {
        Ok(project)
    } else {
        Err(ProjectIoError::Invalid(errors))
    }
}

/// Save a validated project manifest using canonical JSON.
///
/// # Errors
///
/// Returns an error if validation fails, serialization fails, or the manifest
/// cannot be written.
pub fn save_project(project_dir: &Path, project: &Project) -> Result<(), ProjectIoError> {
    let errors = project.validate();
    if !errors.is_empty() {
        return Err(ProjectIoError::Invalid(errors));
    }

    fs::write(project_file_path(project_dir), project.to_canonical_json()?)?;
    Ok(())
}

/// Add a new track through the command log and persist the resulting state.
///
/// # Errors
///
/// Returns an error if the existing project cannot be loaded, command append
/// fails, or the updated project cannot be saved.
pub fn add_track(project_dir: &Path, name: &str) -> Result<Track, ProjectIoError> {
    let track = Track::new(name);
    append_and_apply(
        project_dir,
        ProjectCommand::AddTrack {
            track: track.clone(),
        },
    )?;
    Ok(track)
}

/// Remove a track and its clips through the command log.
///
/// # Errors
///
/// Returns an error if the project cannot be loaded, the track is unknown, or
/// the updated project cannot be saved.
pub fn remove_track(project_dir: &Path, track_id: &StableId) -> Result<Track, ProjectIoError> {
    let project = load_project(project_dir)?;
    let removed = project
        .tracks
        .iter()
        .find(|track| track.id == *track_id)
        .cloned()
        .ok_or_else(|| unknown_track_error(track_id))?;
    append_and_apply(
        project_dir,
        ProjectCommand::RemoveTrack {
            track_id: track_id.clone(),
        },
    )?;
    Ok(removed)
}

/// Add or update a media reference through the command log.
///
/// # Errors
///
/// Returns an error if the project cannot be loaded or updated.
pub fn add_media_reference(
    project_dir: &Path,
    content_hash: &str,
    original_path: Option<String>,
) -> Result<MediaReference, ProjectIoError> {
    let media = MediaReference {
        id: StableId::from_string(content_hash),
        content_hash: Some(content_hash.to_owned()),
        original_path,
    };
    append_and_apply(
        project_dir,
        ProjectCommand::AddMediaReference {
            media: media.clone(),
        },
    )?;
    Ok(media)
}

/// Add a timeline clip to a track.
///
/// # Errors
///
/// Returns an error if the project cannot be loaded, the track/media reference
/// is unknown, or the updated project cannot be saved.
pub fn add_clip(
    project_dir: &Path,
    track_id: &StableId,
    media_id: &StableId,
    start_sample: u64,
    duration_samples: u64,
) -> Result<Clip, ProjectIoError> {
    let clip = Clip {
        id: StableId::new(),
        media_id: media_id.clone(),
        start_sample,
        duration_samples,
    };
    append_and_apply(
        project_dir,
        ProjectCommand::AddClip {
            track_id: track_id.clone(),
            clip: clip.clone(),
        },
    )?;
    Ok(clip)
}

/// Move or resize an existing clip.
///
/// # Errors
///
/// Returns an error if the project cannot be loaded, the clip is unknown, the
/// placement is invalid, or the updated project cannot be saved.
pub fn set_clip_placement(
    project_dir: &Path,
    clip_id: &StableId,
    start_sample: u64,
    duration_samples: u64,
) -> Result<Clip, ProjectIoError> {
    append_and_apply(
        project_dir,
        ProjectCommand::SetClipPlacement {
            clip_id: clip_id.clone(),
            start_sample,
            duration_samples,
        },
    )?;
    let project = load_project(project_dir)?;
    find_clip(&project, clip_id).cloned().ok_or_else(|| {
        ProjectIoError::Invalid(vec![ValidationError::new(format!(
            "unknown clip id {clip_id}"
        ))])
    })
}

/// Remove an existing clip.
///
/// # Errors
///
/// Returns an error if the project cannot be loaded, the clip is unknown, or
/// the updated project cannot be saved.
pub fn remove_clip(project_dir: &Path, clip_id: &StableId) -> Result<Clip, ProjectIoError> {
    let project = load_project(project_dir)?;
    let removed = find_clip(&project, clip_id).cloned().ok_or_else(|| {
        ProjectIoError::Invalid(vec![ValidationError::new(format!(
            "unknown clip id {clip_id}"
        ))])
    })?;
    append_and_apply(
        project_dir,
        ProjectCommand::RemoveClip {
            clip_id: clip_id.clone(),
        },
    )?;
    Ok(removed)
}

/// Set mixer controls for an existing track.
///
/// # Errors
///
/// Returns an error if the project cannot be loaded, the track is unknown, the
/// controls are invalid, or the updated project cannot be saved.
pub fn set_track_controls(
    project_dir: &Path,
    track_id: &StableId,
    volume_percent: u16,
    muted: bool,
    solo: bool,
) -> Result<Track, ProjectIoError> {
    append_and_apply(
        project_dir,
        ProjectCommand::SetTrackControls {
            track_id: track_id.clone(),
            volume_percent,
            muted,
            solo,
        },
    )?;
    let project = load_project(project_dir)?;
    project
        .tracks
        .into_iter()
        .find(|track| track.id == *track_id)
        .ok_or_else(|| {
            ProjectIoError::Invalid(vec![ValidationError::new(format!(
                "unknown track id {track_id}"
            ))])
        })
}

/// Create a branch from the current project state.
///
/// # Errors
///
/// Returns an error if the current project cannot be loaded, the branch name is
/// invalid, or branch state cannot be written.
pub fn create_branch(project_dir: &Path, name: &str) -> Result<Branch, ProjectIoError> {
    validate_branch_name(name)?;
    let project = load_project(project_dir)?;
    save_branch(project_dir, name, &project)?;
    Ok(Branch {
        name: name.to_owned(),
        project,
    })
}

/// List local branch names.
///
/// # Errors
///
/// Returns an error if branch metadata cannot be read or parsed.
pub fn list_branches(project_dir: &Path) -> Result<Vec<String>, ProjectIoError> {
    let branches_path = branch_dir_path(project_dir);
    if !branches_path.exists() {
        return Ok(Vec::new());
    }

    let mut branches = Vec::new();
    for entry in fs::read_dir(branches_path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let json = fs::read_to_string(entry.path())?;
            branches.push(serde_json::from_str::<Branch>(&json)?.name);
        }
    }
    branches.sort();
    Ok(branches)
}

/// Switch the working project to a different local branch.
///
/// # Errors
///
/// Returns an error if current or target branch state cannot be read or written.
pub fn switch_branch(project_dir: &Path, name: &str) -> Result<Project, ProjectIoError> {
    validate_branch_name(name)?;
    persist_current_branch(project_dir)?;
    let branch = load_branch(project_dir, name)?;
    save_project(project_dir, &branch.project)?;
    fs::write(active_branch_path(project_dir), name)?;
    Ok(branch.project)
}

/// Create a full project snapshot and persist it under `history/snapshots`.
///
/// # Errors
///
/// Returns an error if the project cannot be loaded, serialized, or written.
pub fn create_snapshot(project_dir: &Path, message: &str) -> Result<Snapshot, ProjectIoError> {
    fs::create_dir_all(snapshot_dir_path(project_dir))?;
    let project = load_project(project_dir)?;
    let snapshot = Snapshot::new(message, project);
    let mut json = serde_json::to_string_pretty(&snapshot)?;
    json.push('\n');
    fs::write(snapshot_path(project_dir, &snapshot.id), json)?;
    Ok(snapshot)
}

/// Restore a snapshot and append the checkout command to history.
///
/// # Errors
///
/// Returns an error if the snapshot cannot be loaded, command append fails, or
/// the project cannot be saved.
pub fn checkout_snapshot(
    project_dir: &Path,
    snapshot_id: &StableId,
) -> Result<Project, ProjectIoError> {
    let command = ProjectCommand::CheckoutSnapshot {
        snapshot_id: snapshot_id.clone(),
    };
    append_and_apply(project_dir, command)
}

/// Diff two project references.
///
/// References are `current`, `snapshot:<id>`, `<snapshot-id>`, or `branch:<name>`.
///
/// # Errors
///
/// Returns an error if either reference cannot be read or parsed.
pub fn diff(project_dir: &Path, left: &str, right: &str) -> Result<ProjectDiff, ProjectIoError> {
    let left_project = load_project_ref(project_dir, left)?;
    let right_project = load_project_ref(project_dir, right)?;
    Ok(diff_projects(&left_project, &right_project))
}

/// Merge a local branch into the current branch.
///
/// # Errors
///
/// Returns conflicts if matching stable IDs have different track content, or an
/// error if branch/project state cannot be read or written.
pub fn merge_branch(
    project_dir: &Path,
    source_branch: &str,
) -> Result<MergeReport, ProjectIoError> {
    validate_branch_name(source_branch)?;
    let current = load_project(project_dir)?;
    let source = load_branch(project_dir, source_branch)?;
    let diff = diff_projects(&current, &source.project);

    if !diff.changed_tracks.is_empty() {
        return Err(ProjectIoError::Conflict(diff.changed_tracks));
    }

    for track in &source.project.tracks {
        if !current
            .tracks
            .iter()
            .any(|existing| existing.id == track.id)
        {
            append_and_apply(
                project_dir,
                ProjectCommand::AddTrack {
                    track: track.clone(),
                },
            )?;
        }
    }

    Ok(MergeReport {
        source_branch: source_branch.to_owned(),
        added_tracks: diff.added_tracks,
        conflicts: Vec::new(),
    })
}

/// Load command and snapshot history summaries.
///
/// # Errors
///
/// Returns an error if command or snapshot files cannot be read or parsed.
pub fn history(project_dir: &Path) -> Result<Vec<HistoryItem>, ProjectIoError> {
    let mut items = Vec::new();

    for entry in read_command_log(project_dir)? {
        items.push(HistoryItem {
            id: entry.id,
            summary: entry.command.summary(),
        });
    }

    let snapshots_path = snapshot_dir_path(project_dir);
    if snapshots_path.exists() {
        for entry in fs::read_dir(snapshots_path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let json = fs::read_to_string(entry.path())?;
                let snapshot = serde_json::from_str::<Snapshot>(&json)?;
                items.push(HistoryItem {
                    id: snapshot.id,
                    summary: format!("snapshot '{}'", snapshot.message),
                });
            }
        }
    }

    items.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(items)
}

/// Rebuild project state by replaying `history/base.daw.json` and commands.
///
/// # Errors
///
/// Returns an error if replay inputs cannot be loaded, parsed, or validated.
pub fn replay_project(project_dir: &Path) -> Result<Project, ProjectIoError> {
    let json = fs::read_to_string(base_project_path(project_dir))?;
    let mut project = serde_json::from_str::<Project>(&json)?;

    for entry in read_command_log(project_dir)? {
        project = apply_command(project_dir, project, &entry.command)?;
    }

    let errors = project.validate();
    if errors.is_empty() {
        Ok(project)
    } else {
        Err(ProjectIoError::Invalid(errors))
    }
}

fn append_and_apply(
    project_dir: &Path,
    command: ProjectCommand,
) -> Result<Project, ProjectIoError> {
    let project = load_project(project_dir)?;
    let updated = apply_command(project_dir, project, &command)?;
    append_command(project_dir, &CommandEntry::new(command))?;
    save_project(project_dir, &updated)?;
    persist_current_branch(project_dir)?;
    Ok(updated)
}

fn apply_command(
    project_dir: &Path,
    mut project: Project,
    command: &ProjectCommand,
) -> Result<Project, ProjectIoError> {
    match command {
        ProjectCommand::AddTrack { track } => {
            project.tracks.push(track.clone());
            validate_project_state(project)
        }
        ProjectCommand::RemoveTrack { track_id } => apply_remove_track(project, track_id),
        ProjectCommand::AddMediaReference { media } => {
            if let Some(existing) = project
                .media
                .iter_mut()
                .find(|existing| existing.id == media.id)
            {
                *existing = media.clone();
            } else {
                project.media.push(media.clone());
            }
            validate_project_state(project)
        }
        ProjectCommand::AddClip { track_id, clip } => apply_add_clip(project, track_id, clip),
        ProjectCommand::SetClipPlacement {
            clip_id,
            start_sample,
            duration_samples,
        } => apply_set_clip_placement(project, clip_id, *start_sample, *duration_samples),
        ProjectCommand::RemoveClip { clip_id } => apply_remove_clip(project, clip_id),
        ProjectCommand::SetTrackControls {
            track_id,
            volume_percent,
            muted,
            solo,
        } => apply_set_track_controls(project, track_id, *volume_percent, *muted, *solo),
        ProjectCommand::CheckoutSnapshot { snapshot_id } => {
            Ok(load_snapshot(project_dir, snapshot_id)?.project)
        }
    }
}

fn apply_add_clip(
    mut project: Project,
    track_id: &StableId,
    clip: &Clip,
) -> Result<Project, ProjectIoError> {
    if !project.media.iter().any(|media| media.id == clip.media_id) {
        return Err(ProjectIoError::Invalid(vec![ValidationError::new(
            format!("unknown media id {}", clip.media_id),
        )]));
    }
    let track = project
        .tracks
        .iter_mut()
        .find(|track| track.id == *track_id)
        .ok_or_else(|| unknown_track_error(track_id))?;
    track.clips.push(clip.clone());
    track.clips.sort_by_key(|clip| clip.start_sample);
    validate_project_state(project)
}

fn apply_set_clip_placement(
    mut project: Project,
    clip_id: &StableId,
    start_sample: u64,
    duration_samples: u64,
) -> Result<Project, ProjectIoError> {
    let clip = find_clip_mut(&mut project, clip_id).ok_or_else(|| unknown_clip_error(clip_id))?;
    clip.start_sample = start_sample;
    clip.duration_samples = duration_samples;
    sort_track_clips(&mut project);
    validate_project_state(project)
}

fn apply_remove_clip(mut project: Project, clip_id: &StableId) -> Result<Project, ProjectIoError> {
    let mut removed = false;
    for track in &mut project.tracks {
        let clip_count = track.clips.len();
        track.clips.retain(|clip| clip.id != *clip_id);
        removed |= track.clips.len() != clip_count;
    }
    if removed {
        prune_unused_media_references(&mut project);
        validate_project_state(project)
    } else {
        Err(unknown_clip_error(clip_id))
    }
}

fn apply_remove_track(
    mut project: Project,
    track_id: &StableId,
) -> Result<Project, ProjectIoError> {
    let track_count = project.tracks.len();
    project.tracks.retain(|track| track.id != *track_id);
    if project.tracks.len() == track_count {
        return Err(unknown_track_error(track_id));
    }
    prune_unused_media_references(&mut project);
    validate_project_state(project)
}

fn prune_unused_media_references(project: &mut Project) {
    let used_media_ids = project
        .tracks
        .iter()
        .flat_map(|track| &track.clips)
        .map(|clip| clip.media_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    project
        .media
        .retain(|media| used_media_ids.contains(&media.id));
}

fn apply_set_track_controls(
    mut project: Project,
    track_id: &StableId,
    volume_percent: u16,
    muted: bool,
    solo: bool,
) -> Result<Project, ProjectIoError> {
    let track = project
        .tracks
        .iter_mut()
        .find(|track| track.id == *track_id)
        .ok_or_else(|| unknown_track_error(track_id))?;
    track.volume_percent = volume_percent;
    track.muted = muted;
    track.solo = solo;
    validate_project_state(project)
}

fn validate_project_state(project: Project) -> Result<Project, ProjectIoError> {
    let errors = project.validate();
    if errors.is_empty() {
        Ok(project)
    } else {
        Err(ProjectIoError::Invalid(errors))
    }
}

fn unknown_track_error(track_id: &StableId) -> ProjectIoError {
    ProjectIoError::Invalid(vec![ValidationError::new(format!(
        "unknown track id {track_id}"
    ))])
}

fn unknown_clip_error(clip_id: &StableId) -> ProjectIoError {
    ProjectIoError::Invalid(vec![ValidationError::new(format!(
        "unknown clip id {clip_id}"
    ))])
}

fn find_clip<'a>(project: &'a Project, clip_id: &StableId) -> Option<&'a Clip> {
    project
        .tracks
        .iter()
        .flat_map(|track| &track.clips)
        .find(|clip| clip.id == *clip_id)
}

fn find_clip_mut<'a>(project: &'a mut Project, clip_id: &StableId) -> Option<&'a mut Clip> {
    project
        .tracks
        .iter_mut()
        .flat_map(|track| &mut track.clips)
        .find(|clip| clip.id == *clip_id)
}

fn sort_track_clips(project: &mut Project) {
    for track in &mut project.tracks {
        track.clips.sort_by_key(|clip| clip.start_sample);
    }
}

fn append_command(project_dir: &Path, entry: &CommandEntry) -> Result<(), ProjectIoError> {
    fs::create_dir_all(project_dir.join("history"))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(command_log_path(project_dir))?;
    let line = serde_json::to_string(&entry)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn read_command_log(project_dir: &Path) -> Result<Vec<CommandEntry>, ProjectIoError> {
    let path = command_log_path(project_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if !line.trim().is_empty() {
            entries.push(serde_json::from_str::<CommandEntry>(&line)?);
        }
    }
    Ok(entries)
}

fn load_snapshot(project_dir: &Path, snapshot_id: &StableId) -> Result<Snapshot, ProjectIoError> {
    let json = fs::read_to_string(snapshot_path(project_dir, snapshot_id))?;
    Ok(serde_json::from_str::<Snapshot>(&json)?)
}

fn save_branch(project_dir: &Path, name: &str, project: &Project) -> Result<(), ProjectIoError> {
    fs::create_dir_all(branch_dir_path(project_dir))?;
    let branch = Branch {
        name: name.to_owned(),
        project: project.clone(),
    };
    let mut json = serde_json::to_string_pretty(&branch)?;
    json.push('\n');
    fs::write(branch_path(project_dir, name), json)?;
    Ok(())
}

fn load_branch(project_dir: &Path, name: &str) -> Result<Branch, ProjectIoError> {
    let json = fs::read_to_string(branch_path(project_dir, name))?;
    Ok(serde_json::from_str::<Branch>(&json)?)
}

fn active_branch(project_dir: &Path) -> Result<String, ProjectIoError> {
    let path = active_branch_path(project_dir);
    if !path.exists() {
        return Ok(DEFAULT_BRANCH_NAME.to_owned());
    }
    Ok(fs::read_to_string(path)?.trim().to_owned())
}

fn persist_current_branch(project_dir: &Path) -> Result<(), ProjectIoError> {
    let branch = active_branch(project_dir)?;
    let project = load_project(project_dir)?;
    save_branch(project_dir, &branch, &project)
}

fn validate_branch_name(name: &str) -> Result<(), ProjectIoError> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));

    if valid {
        Ok(())
    } else {
        Err(ProjectIoError::Invalid(vec![ValidationError::new(
            "branch names may contain only ASCII letters, numbers, '-' and '_'",
        )]))
    }
}

fn load_project_ref(project_dir: &Path, reference: &str) -> Result<Project, ProjectIoError> {
    if reference == "current" {
        return load_project(project_dir);
    }

    if let Some(snapshot_id) = reference.strip_prefix("snapshot:") {
        return Ok(load_snapshot(project_dir, &StableId::from_string(snapshot_id))?.project);
    }

    if let Some(branch_name) = reference.strip_prefix("branch:") {
        return Ok(load_branch(project_dir, branch_name)?.project);
    }

    Ok(load_snapshot(project_dir, &StableId::from_string(reference))?.project)
}

fn diff_projects(left: &Project, right: &Project) -> ProjectDiff {
    let added_tracks = right
        .tracks
        .iter()
        .filter(|right_track| {
            !left
                .tracks
                .iter()
                .any(|left_track| left_track.id == right_track.id)
        })
        .map(|track| track.name.clone())
        .collect();
    let removed_tracks = left
        .tracks
        .iter()
        .filter(|left_track| {
            !right
                .tracks
                .iter()
                .any(|right_track| right_track.id == left_track.id)
        })
        .map(|track| track.name.clone())
        .collect();
    let changed_tracks = left
        .tracks
        .iter()
        .filter_map(|left_track| {
            right
                .tracks
                .iter()
                .find(|right_track| right_track.id == left_track.id)
                .filter(|right_track| *right_track != left_track)
                .map(|_| left_track.name.clone())
        })
        .collect();

    ProjectDiff {
        added_tracks,
        removed_tracks,
        changed_tracks,
    }
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn default_track_volume_percent() -> u16 {
    100
}

#[cfg(test)]
mod tests {
    use super::{
        add_clip, add_media_reference, add_track, checkout_snapshot, create_branch,
        create_snapshot, diff, init_project, list_branches, load_project, merge_branch,
        project_file_path, remove_clip, remove_track, replay_project, set_clip_placement,
        set_track_controls, switch_branch, Project, ProjectIoError, StableId, Track,
        PROJECT_SCHEMA_VERSION,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn exposes_package_version() {
        assert!(!super::VERSION.is_empty());
    }

    #[test]
    fn serializes_canonical_project_json() {
        let project = Project {
            schema_version: PROJECT_SCHEMA_VERSION,
            id: StableId::from_string("project-id"),
            name: "Session".to_owned(),
            tracks: vec![Track {
                id: StableId::from_string("track-id"),
                name: "Drums".to_owned(),
                volume_percent: 100,
                muted: false,
                solo: false,
                clips: Vec::new(),
            }],
            media: Vec::new(),
        };

        let json = project.to_canonical_json().expect("serialize project");

        assert_eq!(
            json,
            "{\n  \"schema_version\": 1,\n  \"id\": \"project-id\",\n  \"name\": \"Session\",\n  \"tracks\": [\n    {\n      \"id\": \"track-id\",\n      \"name\": \"Drums\",\n      \"volume_percent\": 100,\n      \"muted\": false,\n      \"solo\": false,\n      \"clips\": []\n    }\n  ],\n  \"media\": []\n}\n"
        );
    }

    #[test]
    fn rejects_duplicate_track_ids() {
        let duplicate_id = StableId::from_string("same-id");
        let project = Project {
            schema_version: PROJECT_SCHEMA_VERSION,
            id: StableId::from_string("project-id"),
            name: "Session".to_owned(),
            tracks: vec![
                Track {
                    id: duplicate_id.clone(),
                    name: "A".to_owned(),
                    volume_percent: 100,
                    muted: false,
                    solo: false,
                    clips: Vec::new(),
                },
                Track {
                    id: duplicate_id,
                    name: "B".to_owned(),
                    volume_percent: 100,
                    muted: false,
                    solo: false,
                    clips: Vec::new(),
                },
            ],
            media: Vec::new(),
        };

        let errors = project.validate();

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message(), "track IDs must be unique");
    }

    #[test]
    fn initializes_expected_project_layout() {
        let project_dir = temp_project_dir("layout");
        let project = init_project(&project_dir, "Layout").expect("init project");

        assert_eq!(project.name, "Layout");
        assert!(project_file_path(&project_dir).is_file());
        assert!(project_dir.join("history").is_dir());
        assert!(project_dir.join("history").join("commands.jsonl").is_file());
        assert!(project_dir.join("history").join("snapshots").is_dir());
        assert!(project_dir.join("history").join("branches").is_dir());
        assert!(project_dir
            .join("history")
            .join("active_branch.txt")
            .is_file());
        assert!(project_dir.join("media").join("objects").is_dir());
        assert!(project_dir.join("cache").is_dir());
        assert!(project_dir.join("exports").is_dir());

        let loaded = load_project(&project_dir).expect("load project");
        assert_eq!(loaded, project);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn adds_track_through_command_log() {
        let project_dir = temp_project_dir("track");
        init_project(&project_dir, "Command Log").expect("init project");

        let track = add_track(&project_dir, "Bass").expect("add track");
        let loaded = load_project(&project_dir).expect("load project");
        let replayed = replay_project(&project_dir).expect("replay project");

        assert_eq!(loaded.tracks, vec![track]);
        assert_eq!(replayed, loaded);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn creates_and_checks_out_snapshot() {
        let project_dir = temp_project_dir("snapshot");
        init_project(&project_dir, "Snapshots").expect("init project");
        add_track(&project_dir, "Original").expect("add original track");
        let snapshot = create_snapshot(&project_dir, "before edits").expect("create snapshot");
        add_track(&project_dir, "Later").expect("add later track");

        let restored = checkout_snapshot(&project_dir, &snapshot.id).expect("checkout snapshot");
        let loaded = load_project(&project_dir).expect("load project");

        assert_eq!(restored.tracks.len(), 1);
        assert_eq!(restored.tracks[0].name, "Original");
        assert_eq!(loaded, restored);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn adds_media_reference_and_clip_through_command_log() {
        let project_dir = temp_project_dir("clip");
        init_project(&project_dir, "Clips").expect("init project");
        let track = add_track(&project_dir, "Audio").expect("add track");
        let media = add_media_reference(&project_dir, "abc123", Some("/tmp/source.wav".to_owned()))
            .expect("add media");

        let clip = add_clip(&project_dir, &track.id, &media.id, 48_000, 24_000).expect("add clip");
        let project = load_project(&project_dir).expect("load project");
        let replayed = replay_project(&project_dir).expect("replay project");

        assert_eq!(project.media, vec![media]);
        assert_eq!(project.tracks[0].clips, vec![clip]);
        assert_eq!(replayed, project);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn sets_track_controls_through_command_log() {
        let project_dir = temp_project_dir("track-controls");
        init_project(&project_dir, "Mixer").expect("init project");
        let track = add_track(&project_dir, "Lead").expect("add track");

        let updated =
            set_track_controls(&project_dir, &track.id, 75, true, false).expect("set controls");
        let project = load_project(&project_dir).expect("load project");
        let replayed = replay_project(&project_dir).expect("replay project");

        assert_eq!(updated.volume_percent, 75);
        assert!(updated.muted);
        assert!(!updated.solo);
        assert_eq!(project.tracks[0], updated);
        assert_eq!(replayed, project);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn edits_and_removes_clips_through_command_log() {
        let project_dir = temp_project_dir("clip-edit");
        init_project(&project_dir, "Clip Edits").expect("init project");
        let track = add_track(&project_dir, "Audio").expect("add track");
        let media = add_media_reference(&project_dir, "abc123", Some("/tmp/source.wav".to_owned()))
            .expect("add media");
        let first_clip =
            add_clip(&project_dir, &track.id, &media.id, 48_000, 24_000).expect("add first clip");
        let second_clip =
            add_clip(&project_dir, &track.id, &media.id, 96_000, 12_000).expect("add second clip");

        let edited =
            set_clip_placement(&project_dir, &second_clip.id, 12_000, 36_000).expect("edit clip");
        let removed = remove_clip(&project_dir, &first_clip.id).expect("remove clip");
        let project = load_project(&project_dir).expect("load project");
        let replayed = replay_project(&project_dir).expect("replay project");

        assert_eq!(edited.start_sample, 12_000);
        assert_eq!(edited.duration_samples, 36_000);
        assert_eq!(removed, first_clip);
        assert_eq!(project.tracks[0].clips, vec![edited]);
        assert_eq!(project.media, vec![media]);
        assert_eq!(replayed, project);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn removes_unused_media_when_last_clip_is_removed() {
        let project_dir = temp_project_dir("clip-media-prune");
        init_project(&project_dir, "Clip Media Prune").expect("init project");
        let track = add_track(&project_dir, "Audio").expect("add track");
        let media = add_media_reference(&project_dir, "abc123", Some("/tmp/source.wav".to_owned()))
            .expect("add media");
        let clip = add_clip(&project_dir, &track.id, &media.id, 0, 24_000).expect("add clip");

        remove_clip(&project_dir, &clip.id).expect("remove clip");
        let project = load_project(&project_dir).expect("load project");
        let replayed = replay_project(&project_dir).expect("replay project");

        assert!(project.tracks[0].clips.is_empty());
        assert!(project.media.is_empty());
        assert_eq!(replayed, project);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn removes_track_and_prunes_its_media() {
        let project_dir = temp_project_dir("track-remove");
        init_project(&project_dir, "Track Remove").expect("init project");
        let first = add_track(&project_dir, "First").expect("add first track");
        let second = add_track(&project_dir, "Second").expect("add second track");
        let first_media =
            add_media_reference(&project_dir, "abc123", Some("/tmp/first.wav".to_owned()))
                .expect("add first media");
        let second_media =
            add_media_reference(&project_dir, "def456", Some("/tmp/second.wav".to_owned()))
                .expect("add second media");
        add_clip(&project_dir, &first.id, &first_media.id, 0, 24_000).expect("add first clip");
        let second_clip = add_clip(&project_dir, &second.id, &second_media.id, 48_000, 12_000)
            .expect("add second clip");

        let removed = remove_track(&project_dir, &first.id).expect("remove track");
        let project = load_project(&project_dir).expect("load project");
        let replayed = replay_project(&project_dir).expect("replay project");

        assert_eq!(removed.id, first.id);
        assert_eq!(removed.name, first.name);
        assert_eq!(removed.clips.len(), 1);
        assert_eq!(
            project.tracks,
            vec![Track {
                clips: vec![second_clip],
                ..second
            }]
        );
        assert_eq!(project.media, vec![second_media]);
        assert_eq!(replayed, project);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn creates_and_switches_branches() {
        let project_dir = temp_project_dir("branches");
        init_project(&project_dir, "Branches").expect("init project");
        add_track(&project_dir, "Drums").expect("add drums");
        create_branch(&project_dir, "chorus").expect("create branch");
        add_track(&project_dir, "Bass").expect("add bass");

        switch_branch(&project_dir, "chorus").expect("switch branch");
        let project = load_project(&project_dir).expect("load project");
        let branches = list_branches(&project_dir).expect("list branches");

        assert_eq!(project.tracks.len(), 1);
        assert_eq!(project.tracks[0].name, "Drums");
        assert_eq!(branches, vec!["chorus".to_owned(), "main".to_owned()]);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn diffs_and_merges_non_conflicting_branch_tracks() {
        let project_dir = temp_project_dir("merge");
        init_project(&project_dir, "Merge").expect("init project");
        add_track(&project_dir, "Drums").expect("add drums");
        create_branch(&project_dir, "feature").expect("create branch");
        switch_branch(&project_dir, "feature").expect("switch feature");
        add_track(&project_dir, "Lead").expect("add lead");
        switch_branch(&project_dir, "main").expect("switch main");
        add_track(&project_dir, "Bass").expect("add bass");

        let diff = diff(&project_dir, "current", "branch:feature").expect("diff branch");
        let report = merge_branch(&project_dir, "feature").expect("merge branch");
        let project = load_project(&project_dir).expect("load project");

        assert_eq!(diff.added_tracks, vec!["Lead".to_owned()]);
        assert_eq!(diff.removed_tracks, vec!["Bass".to_owned()]);
        assert_eq!(report.added_tracks, vec!["Lead".to_owned()]);
        assert_eq!(project.tracks.len(), 3);

        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    #[test]
    fn load_rejects_invalid_project() {
        let project_dir = temp_project_dir("invalid");
        fs::create_dir_all(&project_dir).expect("create project dir");
        fs::write(
            project_file_path(&project_dir),
            "{\n  \"schema_version\": 999,\n  \"id\": \"project-id\",\n  \"name\": \"Broken\",\n  \"tracks\": [],\n  \"media\": []\n}\n",
        )
        .expect("write manifest");

        let result = load_project(&project_dir);

        assert!(matches!(result, Err(ProjectIoError::Invalid(_))));
        fs::remove_dir_all(project_dir).expect("cleanup project");
    }

    fn temp_project_dir(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("daw-model-{test_name}-{}", StableId::new()))
    }
}
