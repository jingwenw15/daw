//! Native desktop UI shell for the DAW.

use eframe::egui::{self, scroll_area::ScrollSource};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const TRACK_HEADER_WIDTH: f32 = 260.0;
const TRACK_LANE_HEIGHT: f32 = 92.0;
const MIN_TIMELINE_SAMPLES: u64 = 240_000;
const DEFAULT_SNAP_GRID_MS: u32 = 250;
const DEFAULT_BEAT_DIVISION: u16 = 1;
const BEATS_PER_BAR: u64 = 4;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1120.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native("DAW", options, Box::new(|_| Ok(Box::<DawApp>::default())))
}

#[allow(clippy::struct_excessive_bools)]
struct DawApp {
    project_path: String,
    new_project_name: String,
    new_track_name: String,
    project_tempo_bpm: u16,
    playhead_sample: String,
    media_source_path: String,
    clip_track_id: String,
    clip_media_id: String,
    clip_start_sample: String,
    clip_duration_samples: String,
    edit_clip_id: String,
    edit_clip_start_sample: String,
    edit_clip_duration_samples: String,
    recording_track_id: String,
    recording_start_sample: String,
    mixer_track_id: String,
    mixer_volume_percent: String,
    mixer_muted: bool,
    mixer_solo: bool,
    timeline_zoom: f32,
    metronome_enabled: bool,
    snap_enabled: bool,
    snap_mode: SnapMode,
    snap_grid_ms: u32,
    snap_beat_division: u16,
    selected_clip_id: Option<daw_model::StableId>,
    clip_drag: Option<ActiveClipDrag>,
    track_name_edits: BTreeMap<String, String>,
    snapshot_message: String,
    status: String,
    project: Option<daw_model::Project>,
    media: Vec<daw_media::MediaObject>,
    waveforms: Vec<daw_media::WaveformSummary>,
    history: Vec<daw_model::HistoryItem>,
    playback: Option<ActivePlayback>,
    recording: Option<ActiveRecording>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordingInsertReport {
    message: String,
    media_hash: String,
}

struct ActiveRecording {
    transport: daw_engine::RecordingTransport,
    metronome: Option<daw_engine::PlaybackTransport>,
    track_id: daw_model::StableId,
    start_sample: u64,
}

struct ActivePlayback {
    transport: daw_engine::PlaybackTransport,
    start_sample: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SnapMode {
    Time,
    Beat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TimelineGrid {
    snap: Option<u64>,
    beat: Option<u64>,
    bar: Option<u64>,
}

#[derive(Clone, Debug)]
struct ActiveClipDrag {
    clip_id: daw_model::StableId,
    original_track_id: daw_model::StableId,
    current_track_id: daw_model::StableId,
    original_start_sample: u64,
    duration_samples: u64,
    current_start_sample: u64,
    start_pointer_x: f32,
}

#[derive(Clone, Debug)]
struct LiveRecordingPreview {
    track_id: daw_model::StableId,
    start_sample: u64,
    duration_samples: u64,
    peaks: Vec<daw_media::WaveformPeak>,
}

#[derive(Clone, Debug)]
struct ClipMoveRequest {
    clip_id: daw_model::StableId,
    track_id: daw_model::StableId,
    start_sample: u64,
    duration_samples: u64,
}

struct ClipRenderResult {
    rect: egui::Rect,
    action: Option<ArrangementAction>,
}

#[derive(Clone, Debug)]
enum ArrangementAction {
    BeginClipDrag {
        clip_id: daw_model::StableId,
        track_id: daw_model::StableId,
        start_sample: u64,
        duration_samples: u64,
        pointer_x: f32,
    },
    UpdateClipDrag {
        track_id: daw_model::StableId,
        pointer_x: f32,
        lane_width: f32,
        timeline_samples: u64,
        snap_grid_samples: Option<u64>,
    },
    EndClipDrag,
    SetPlayhead(u64),
    SelectClip(daw_model::StableId),
    ArmTrack(daw_model::StableId),
    RemoveTrack(daw_model::StableId),
    RenameTrack {
        track_id: daw_model::StableId,
        name: String,
    },
    SetTrackControls {
        track_id: daw_model::StableId,
        volume_percent: u16,
        muted: bool,
        solo: bool,
    },
}

impl Default for DawApp {
    fn default() -> Self {
        Self {
            project_path: "/private/tmp/daw-ui-project".to_owned(),
            new_project_name: "UI Project".to_owned(),
            new_track_name: "Audio".to_owned(),
            project_tempo_bpm: daw_model::DEFAULT_TEMPO_BPM,
            playhead_sample: "0".to_owned(),
            media_source_path: "/private/tmp/test-tone.wav".to_owned(),
            clip_track_id: String::new(),
            clip_media_id: String::new(),
            clip_start_sample: "0".to_owned(),
            clip_duration_samples: "48000".to_owned(),
            edit_clip_id: String::new(),
            edit_clip_start_sample: "0".to_owned(),
            edit_clip_duration_samples: "48000".to_owned(),
            recording_track_id: String::new(),
            recording_start_sample: "0".to_owned(),
            mixer_track_id: String::new(),
            mixer_volume_percent: "100".to_owned(),
            mixer_muted: false,
            mixer_solo: false,
            timeline_zoom: 1.0,
            metronome_enabled: false,
            snap_enabled: true,
            snap_mode: SnapMode::Time,
            snap_grid_ms: DEFAULT_SNAP_GRID_MS,
            snap_beat_division: DEFAULT_BEAT_DIVISION,
            selected_clip_id: None,
            clip_drag: None,
            track_name_edits: BTreeMap::new(),
            snapshot_message: "UI snapshot".to_owned(),
            status: "No project loaded".to_owned(),
            project: None,
            media: Vec::new(),
            waveforms: Vec::new(),
            history: Vec::new(),
            playback: None,
            recording: None,
        }
    }
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll_playback();
        self.handle_shortcuts(ctx);
        if self.recording.is_some() || self.playback.is_some() || self.clip_drag.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
        self.render_transport(ctx);
        self.render_utilities(ctx);
        self.render_project(ctx);
    }
}

impl DawApp {
    #[allow(clippy::too_many_lines)]
    fn render_transport(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("transport").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("DAW");
                ui.add_sized(
                    [280.0, 24.0],
                    egui::TextEdit::singleline(&mut self.project_path),
                );
                if ui.button("Create").clicked() {
                    self.create_project();
                }
                if ui.button("Open").clicked() {
                    self.reload_project();
                }
                ui.separator();
                ui.label("Playhead");
                ui.add_sized(
                    [96.0, 24.0],
                    egui::TextEdit::singleline(&mut self.playhead_sample),
                );
                ui.label("Zoom");
                ui.add(
                    egui::Slider::new(&mut self.timeline_zoom, 0.5..=8.0)
                        .logarithmic(true)
                        .show_value(false),
                );
                ui.label("Tempo");
                ui.add(
                    egui::DragValue::new(&mut self.project_tempo_bpm)
                        .range(20..=300)
                        .speed(1.0)
                        .suffix(" BPM"),
                );
                if ui.button("Set Tempo").clicked() {
                    self.set_project_tempo();
                }
                ui.checkbox(&mut self.metronome_enabled, "Metronome");
                ui.checkbox(&mut self.snap_enabled, "Snap");
                ui.add_enabled_ui(self.snap_enabled, |ui| {
                    egui::ComboBox::from_id_salt("snap-mode")
                        .selected_text(match self.snap_mode {
                            SnapMode::Time => "Time",
                            SnapMode::Beat => "Beat",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.snap_mode, SnapMode::Time, "Time");
                            ui.selectable_value(&mut self.snap_mode, SnapMode::Beat, "Beat");
                        });
                    match self.snap_mode {
                        SnapMode::Time => {
                            ui.add(
                                egui::DragValue::new(&mut self.snap_grid_ms)
                                    .range(10..=2_000)
                                    .speed(10.0)
                                    .suffix(" ms"),
                            );
                        }
                        SnapMode::Beat => {
                            ui.label("Div");
                            ui.add(
                                egui::DragValue::new(&mut self.snap_beat_division)
                                    .range(1..=16)
                                    .speed(1.0)
                                    .prefix("1/")
                                    .suffix(" beat"),
                            );
                        }
                    }
                });
                if ui
                    .add_enabled(self.playback.is_none(), egui::Button::new("Play"))
                    .clicked()
                {
                    self.play_project();
                }
                if ui
                    .add_enabled(self.playback.is_some(), egui::Button::new("Stop"))
                    .clicked()
                {
                    self.stop_playback();
                }
                let record_label = if self.recording.is_some() {
                    "Stop Recording"
                } else {
                    "Record"
                };
                let record_button =
                    egui::Button::new(record_label).fill(if self.recording.is_some() {
                        egui::Color32::from_rgb(178, 40, 48)
                    } else {
                        egui::Color32::from_rgb(114, 28, 36)
                    });
                if ui.add(record_button).clicked() {
                    if self.recording.is_some() {
                        self.stop_recording();
                    } else {
                        self.start_recording();
                    }
                }
                ui.separator();
                ui.add_sized(
                    [140.0, 24.0],
                    egui::TextEdit::singleline(&mut self.new_track_name),
                );
                if ui.button("Add Track").clicked() {
                    self.add_track();
                }
            });
        });
    }

    fn render_utilities(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("utilities")
            .resizable(true)
            .default_height(72.0)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(&self.status);
                    if let Some(project) = &self.project {
                        if let Some(clip) = selected_clip(project, self.selected_clip_id.as_ref()) {
                            ui.separator();
                            ui.label(format!(
                                "Selected clip {}  start {}  duration {}",
                                clip.id, clip.start_sample, clip.duration_samples
                            ));
                            if ui.button("Delete Clip").clicked() {
                                self.remove_selected_clip();
                            }
                        }
                    }
                    ui.separator();
                    if ui.button("Validate").clicked() {
                        self.validate_project();
                    }
                    if ui.button("Generate Waveforms").clicked() {
                        self.generate_waveforms();
                    }
                });
                ui.collapsing("Advanced project tools", |ui| {
                    ui.columns(4, |columns| {
                        columns[0].heading("Project");
                        self.render_project_edit_section(&mut columns[0]);
                        columns[1].heading("Media");
                        self.render_media_clip_section(&mut columns[1]);
                        columns[2].heading("Clip");
                        self.render_clip_edit_section(&mut columns[2]);
                        columns[3].heading("Snapshots");
                        self.render_snapshot_section(&mut columns[3]);
                    });
                });
            });
    }

    fn render_project_edit_section(&mut self, ui: &mut egui::Ui) {
        ui.label("New project name");
        ui.text_edit_singleline(&mut self.new_project_name);
        ui.separator();
        ui.label("New track");
        ui.text_edit_singleline(&mut self.new_track_name);
        if ui.button("Add Track").clicked() {
            self.add_track();
        }
        ui.separator();
    }

    fn render_media_clip_section(&mut self, ui: &mut egui::Ui) {
        ui.label("Media source path");
        ui.text_edit_singleline(&mut self.media_source_path);
        if ui.button("Import Media").clicked() {
            self.import_media();
        }
        if ui.button("Generate Waveforms").clicked() {
            self.generate_waveforms();
        }
        ui.separator();
        ui.label("Clip track id");
        ui.text_edit_singleline(&mut self.clip_track_id);
        ui.label("Clip media id");
        ui.text_edit_singleline(&mut self.clip_media_id);
        ui.horizontal(|ui| {
            ui.label("Start");
            ui.text_edit_singleline(&mut self.clip_start_sample);
            ui.label("Duration");
            ui.text_edit_singleline(&mut self.clip_duration_samples);
        });
        ui.horizontal(|ui| {
            if ui.button("Use First IDs").clicked() {
                self.use_first_clip_ids();
            }
            if ui.button("Add Clip").clicked() {
                self.add_clip();
            }
        });
        ui.separator();
    }

    fn render_clip_edit_section(&mut self, ui: &mut egui::Ui) {
        ui.label("Edit clip id");
        ui.text_edit_singleline(&mut self.edit_clip_id);
        ui.horizontal(|ui| {
            ui.label("Start");
            ui.text_edit_singleline(&mut self.edit_clip_start_sample);
            ui.label("Duration");
            ui.text_edit_singleline(&mut self.edit_clip_duration_samples);
        });
        ui.horizontal(|ui| {
            if ui.button("Use First Clip").clicked() {
                self.use_first_clip();
            }
            if ui.button("Move Clip").clicked() {
                self.move_clip();
            }
            if ui.button("Remove Clip").clicked() {
                self.remove_clip();
            }
        });
        ui.separator();
    }

    fn render_snapshot_section(&mut self, ui: &mut egui::Ui) {
        ui.label("Snapshot message");
        ui.text_edit_singleline(&mut self.snapshot_message);
        if ui.button("Create Snapshot").clicked() {
            self.create_snapshot();
        }
        ui.separator();
    }

    fn render_project(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(project) = self.project.clone() {
                self.sync_track_name_edits(&project);
                ui.horizontal(|ui| {
                    ui.heading(&project.name);
                    if self.recording.is_some() {
                        ui.colored_label(egui::Color32::from_rgb(220, 72, 82), "recording");
                    }
                    ui.label(format!("{} BPM", project.tempo_bpm));
                    ui.label(format!("{} tracks", project.tracks.len()));
                    ui.label(format!("{} media", project.media.len()));
                });
                ui.separator();
                let live_recording = self.live_recording_preview();
                let actions = render_arrangement(
                    ui,
                    &project,
                    &self.media,
                    &self.waveforms,
                    &self.playhead_sample,
                    live_recording.as_ref(),
                    self.timeline_zoom,
                    self.timeline_grid(),
                    self.selected_clip_id.as_ref(),
                    self.clip_drag.as_ref(),
                    &self.recording_track_id,
                    &mut self.track_name_edits,
                );
                for action in actions {
                    self.apply_arrangement_action(&action);
                }
            } else {
                ui.heading("No project loaded");
                ui.label("Create or open a project, add a track, then press Record.");
            }
        });
    }

    fn timeline_grid(&self) -> TimelineGrid {
        if !self.snap_enabled {
            return TimelineGrid {
                snap: None,
                beat: None,
                bar: None,
            };
        }
        match self.snap_mode {
            SnapMode::Time => TimelineGrid {
                snap: Some(snap_grid_samples_from_ms(self.snap_grid_ms)),
                beat: None,
                bar: None,
            },
            SnapMode::Beat => {
                let beat_samples = samples_per_beat(self.project_tempo_bpm);
                TimelineGrid {
                    snap: Some(beat_samples / u64::from(self.snap_beat_division).max(1)),
                    beat: Some(beat_samples),
                    bar: Some(beat_samples.saturating_mul(BEATS_PER_BAR)),
                }
            }
        }
    }

    fn create_project(&mut self) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::init_project(&path, &self.new_project_name) {
            Ok(project) => {
                self.project_tempo_bpm = project.tempo_bpm;
                self.project = Some(project);
                self.clip_drag = None;
                self.status = format!("Created project at {}", path.display());
                self.reload_auxiliary();
            }
            Err(error) => self.status = format!("Create failed: {error}"),
        }
    }

    fn reload_project(&mut self) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::load_project(&path) {
            Ok(project) => {
                self.project_tempo_bpm = project.tempo_bpm;
                self.project = Some(project);
                self.clip_drag = None;
                self.status = format!("Opened {}", path.display());
                self.reload_auxiliary();
            }
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    fn refresh_project_after_edit(&mut self, message: impl Into<String>) {
        let message = message.into();
        match self.load_and_verify_project() {
            Ok(()) => self.status = format!("{message} · verified after reload"),
            Err(error) => self.status = format!("{message} · verification failed: {error}"),
        }
    }

    fn load_and_verify_project(&mut self) -> Result<(), String> {
        let path = PathBuf::from(&self.project_path);
        let project = daw_model::load_project(&path).map_err(|error| error.to_string())?;
        let replayed = daw_model::replay_project(&path).map_err(|error| error.to_string())?;
        if replayed != project {
            return Err("command replay differs from saved project".to_owned());
        }
        self.project_tempo_bpm = project.tempo_bpm;
        self.project = Some(project);
        self.clip_drag = None;
        self.reload_auxiliary();
        Ok(())
    }

    fn reload_auxiliary(&mut self) {
        let path = PathBuf::from(&self.project_path);
        self.media = daw_media::list_media(&path).unwrap_or_default();
        self.waveforms = self
            .media
            .iter()
            .filter_map(|media| daw_media::load_waveform(&path, &media.hash).ok())
            .collect();
        self.history = daw_model::history(&path).unwrap_or_default();
    }

    fn validate_project(&mut self) {
        let path = PathBuf::from(&self.project_path);
        self.status = match daw_model::load_project(&path) {
            Ok(_) => "Project is valid".to_owned(),
            Err(error) => format!("Validation failed: {error}"),
        };
    }

    fn add_track(&mut self) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::add_track(&path, &self.new_track_name) {
            Ok(track) => {
                self.clip_track_id = track.id.to_string();
                self.mixer_track_id = track.id.to_string();
                self.recording_track_id = track.id.to_string();
                self.refresh_project_after_edit(format!("Added track '{}'", track.name));
            }
            Err(error) => self.status = format!("Add track failed: {error}"),
        }
    }

    fn set_project_tempo(&mut self) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::set_project_tempo(&path, self.project_tempo_bpm) {
            Ok(project) => {
                self.project_tempo_bpm = project.tempo_bpm;
                self.refresh_project_after_edit(format!(
                    "Set project tempo to {} BPM",
                    project.tempo_bpm
                ));
            }
            Err(error) => self.status = format!("Set tempo failed: {error}"),
        }
    }

    fn import_media(&mut self) {
        let path = PathBuf::from(&self.project_path);
        let object = match daw_media::import_media(&path, Path::new(&self.media_source_path)) {
            Ok(object) => object,
            Err(error) => {
                self.status = format!("Import failed: {error}");
                return;
            }
        };

        match daw_model::add_media_reference(
            &path,
            &object.hash,
            Some(object.original_path.clone()),
        ) {
            Ok(media) => {
                self.clip_media_id = media.id.to_string();
                self.refresh_project_after_edit(format!(
                    "Imported {} bytes as {}",
                    object.byte_size, object.hash
                ));
            }
            Err(error) => self.status = format!("Media registration failed: {error}"),
        }
    }

    fn generate_waveforms(&mut self) {
        let path = PathBuf::from(&self.project_path);
        match daw_media::generate_waveforms(&path, daw_media::DEFAULT_WAVEFORM_POINTS) {
            Ok(waveforms) => {
                self.status = format!("Generated {} waveform caches", waveforms.len());
                self.reload_auxiliary();
            }
            Err(error) => self.status = format!("Waveform generation failed: {error}"),
        }
    }

    fn use_first_clip_ids(&mut self) {
        if let Some(project) = &self.project {
            let track = project.tracks.first().cloned();
            let media_id = project.media.first().map(|media| media.id.to_string());
            if let Some(track) = track {
                self.clip_track_id = track.id.to_string();
                self.recording_track_id = track.id.to_string();
                self.set_mixer_fields_from_track(&track);
            }
            if let Some(media_id) = media_id {
                self.clip_media_id = media_id;
            }
            "Selected first track/media IDs".clone_into(&mut self.status);
        } else {
            "Open a project before selecting IDs".clone_into(&mut self.status);
        }
    }

    fn set_mixer_fields_from_track(&mut self, track: &daw_model::Track) {
        self.mixer_track_id = track.id.to_string();
        self.mixer_volume_percent = track.volume_percent.to_string();
        self.mixer_muted = track.muted;
        self.mixer_solo = track.solo;
    }

    fn add_clip(&mut self) {
        let path = PathBuf::from(&self.project_path);
        let start_sample = match parse_u64(&self.clip_start_sample, "clip start") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let duration_samples = match parse_u64(&self.clip_duration_samples, "clip duration") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        match daw_model::add_clip(
            &path,
            &daw_model::StableId::from_string(self.clip_track_id.clone()),
            &daw_model::StableId::from_string(self.clip_media_id.clone()),
            start_sample,
            duration_samples,
        ) {
            Ok(clip) => {
                self.set_clip_edit_fields(&clip);
                self.refresh_project_after_edit(format!("Added clip {}", clip.id));
            }
            Err(error) => self.status = format!("Add clip failed: {error}"),
        }
    }

    fn create_snapshot(&mut self) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::create_snapshot(&path, &self.snapshot_message) {
            Ok(snapshot) => {
                self.status = format!("Created snapshot '{}'", snapshot.message);
                self.reload_auxiliary();
            }
            Err(error) => self.status = format!("Snapshot failed: {error}"),
        }
    }

    fn use_first_recording_track(&mut self) {
        let track_id = self
            .project
            .as_ref()
            .and_then(|project| project.tracks.first())
            .map(|track| track.id.to_string());
        if let Some(track_id) = track_id {
            self.recording_track_id = track_id;
            "Selected first recording track".clone_into(&mut self.status);
        } else if self.project.is_some() {
            "Project has no tracks".clone_into(&mut self.status);
        } else {
            "Open a project before selecting a recording track".clone_into(&mut self.status);
        }
    }

    fn start_recording(&mut self) {
        if self.recording.is_some() {
            "Already recording".clone_into(&mut self.status);
            return;
        }
        if self.recording_track_id.is_empty() {
            self.use_first_recording_track();
        }
        self.recording_start_sample
            .clone_from(&self.playhead_sample);

        let start_sample = match parse_u64(&self.recording_start_sample, "recording start") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if self.recording_track_id.is_empty() {
            "Add a track before recording".clone_into(&mut self.status);
            return;
        }
        match daw_engine::start_input_recording() {
            Ok(transport) => {
                let metronome = if self.metronome_enabled {
                    match self.start_recording_metronome() {
                        Ok(metronome) => Some(metronome),
                        Err(error) => {
                            self.status = format!("Metronome unavailable: {error}");
                            None
                        }
                    }
                } else {
                    None
                };
                let metronome_status = if metronome.is_some() {
                    " with metronome"
                } else {
                    ""
                };
                self.status = format!(
                    "Recording from '{}'{}",
                    transport.report().device_name,
                    metronome_status
                );
                self.recording = Some(ActiveRecording {
                    transport,
                    metronome,
                    track_id: daw_model::StableId::from_string(self.recording_track_id.clone()),
                    start_sample,
                });
            }
            Err(error) => self.status = format!("Record start failed: {error}"),
        }
    }

    fn stop_recording(&mut self) {
        let Some(mut recording) = self.recording.take() else {
            "Nothing is recording".clone_into(&mut self.status);
            return;
        };
        if let Some(mut metronome) = recording.metronome.take() {
            let _ = metronome.stop();
        }
        match recording.transport.stop() {
            Ok(recorded) => match insert_recorded_audio(
                Path::new(&self.project_path),
                &recording.track_id,
                recording.start_sample,
                &recorded,
            ) {
                Ok(report) => {
                    let path = PathBuf::from(&self.project_path);
                    let _ = daw_media::generate_waveform(
                        &path,
                        &report.media_hash,
                        daw_media::DEFAULT_WAVEFORM_POINTS,
                    );
                    self.refresh_project_after_edit(report.message);
                }
                Err(error) => self.status = format!("Record insert failed: {error}"),
            },
            Err(error) => self.status = format!("Record stop failed: {error}"),
        }
    }

    fn live_recording_preview(&mut self) -> Option<LiveRecordingPreview> {
        let recording = self.recording.as_ref()?;
        let recorded = recording.transport.snapshot().ok()?;
        let duration_samples = u64::try_from(recorded.buffer.frames()).ok()?;
        let playhead = recording.start_sample.saturating_add(duration_samples);
        self.playhead_sample = playhead.to_string();
        self.recording_start_sample = self.playhead_sample.clone();
        Some(LiveRecordingPreview {
            track_id: recording.track_id.clone(),
            start_sample: recording.start_sample,
            duration_samples,
            peaks: waveform_peaks_from_buffer(&recorded.buffer, daw_media::DEFAULT_WAVEFORM_POINTS),
        })
    }

    fn start_recording_metronome(&self) -> Result<daw_engine::PlaybackTransport, String> {
        let buffer = daw_engine::render_metronome(
            self.project_tempo_bpm,
            u16::try_from(BEATS_PER_BAR).unwrap_or(4),
            1,
            daw_engine::DEFAULT_SAMPLE_RATE,
            daw_engine::DEFAULT_CHANNELS,
        )
        .map_err(|error| format!("Metronome render failed: {error}"))?;
        daw_engine::start_looping_buffer_playback(buffer)
            .map_err(|error| format!("Metronome playback failed: {error}"))
    }

    fn commit_clip_move(&mut self, request: &ClipMoveRequest) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::set_clip_placement_on_track(
            &path,
            &request.clip_id,
            Some(&request.track_id),
            request.start_sample,
            request.duration_samples,
        ) {
            Ok(clip) => {
                let message = format!(
                    "Moved clip {} to track {} at {}",
                    clip.id, request.track_id, clip.start_sample
                );
                self.set_clip_edit_fields(&clip);
                self.refresh_project_after_edit(message);
            }
            Err(error) => self.status = format!("Move clip failed: {error}"),
        }
    }

    fn apply_arrangement_action(&mut self, action: &ArrangementAction) {
        match action {
            ArrangementAction::BeginClipDrag {
                clip_id,
                track_id,
                start_sample,
                duration_samples,
                pointer_x,
            } => {
                self.selected_clip_id = Some(clip_id.clone());
                self.edit_clip_id = clip_id.to_string();
                self.edit_clip_start_sample = start_sample.to_string();
                self.edit_clip_duration_samples = duration_samples.to_string();
                self.clip_drag = Some(ActiveClipDrag {
                    clip_id: clip_id.clone(),
                    original_track_id: track_id.clone(),
                    current_track_id: track_id.clone(),
                    original_start_sample: *start_sample,
                    duration_samples: *duration_samples,
                    current_start_sample: *start_sample,
                    start_pointer_x: *pointer_x,
                });
                self.status = format!("Dragging clip {clip_id}");
            }
            ArrangementAction::UpdateClipDrag {
                track_id,
                pointer_x,
                lane_width,
                timeline_samples,
                snap_grid_samples,
            } => {
                if let Some(drag) = &mut self.clip_drag {
                    drag.current_track_id = track_id.clone();
                    let delta_pixels = *pointer_x - drag.start_pointer_x;
                    let delta_samples =
                        pixels_to_samples(delta_pixels, *lane_width, *timeline_samples);
                    let current_start_sample =
                        apply_sample_delta(drag.original_start_sample, delta_samples);
                    drag.current_start_sample =
                        snap_sample(current_start_sample, *snap_grid_samples);
                    self.edit_clip_start_sample = drag.current_start_sample.to_string();
                    self.playhead_sample = drag.current_start_sample.to_string();
                }
            }
            ArrangementAction::EndClipDrag => {
                if let Some(drag) = self.clip_drag.take() {
                    if drag.current_start_sample != drag.original_start_sample
                        || drag.current_track_id != drag.original_track_id
                    {
                        self.commit_clip_move(&ClipMoveRequest {
                            clip_id: drag.clip_id,
                            track_id: drag.current_track_id,
                            start_sample: drag.current_start_sample,
                            duration_samples: drag.duration_samples,
                        });
                    }
                }
            }
            ArrangementAction::SetPlayhead(sample) => {
                self.playhead_sample = sample.to_string();
                self.recording_start_sample = self.playhead_sample.clone();
                self.status = format!("Playhead set to {sample}");
            }
            ArrangementAction::SelectClip(clip_id) => {
                self.selected_clip_id = Some(clip_id.clone());
                self.edit_clip_id = clip_id.to_string();
                if let Some(clip) = self
                    .project
                    .as_ref()
                    .and_then(|project| selected_clip(project, self.selected_clip_id.as_ref()))
                    .cloned()
                {
                    self.set_clip_edit_fields(&clip);
                }
            }
            ArrangementAction::ArmTrack(track_id) => {
                self.recording_track_id = track_id.to_string();
                self.status = format!("Armed track {track_id}");
            }
            ArrangementAction::RemoveTrack(track_id) => self.remove_track(track_id),
            ArrangementAction::RenameTrack { track_id, name } => {
                self.rename_track(track_id, name);
            }
            ArrangementAction::SetTrackControls {
                track_id,
                volume_percent,
                muted,
                solo,
            } => self.commit_track_controls(track_id, *volume_percent, *muted, *solo),
        }
    }

    fn commit_track_controls(
        &mut self,
        track_id: &daw_model::StableId,
        volume_percent: u16,
        muted: bool,
        solo: bool,
    ) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::set_track_controls(&path, track_id, volume_percent, muted, solo) {
            Ok(track) => {
                let message = format!(
                    "Set '{}' controls: volume={} muted={} solo={}",
                    track.name, track.volume_percent, track.muted, track.solo
                );
                self.set_mixer_fields_from_track(&track);
                self.refresh_project_after_edit(message);
            }
            Err(error) => self.status = format!("Set controls failed: {error}"),
        }
    }

    fn rename_track(&mut self, track_id: &daw_model::StableId, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            "Track name cannot be empty".clone_into(&mut self.status);
            return;
        }
        let path = PathBuf::from(&self.project_path);
        match daw_model::set_track_name(&path, track_id, name) {
            Ok(track) => {
                let message = format!("Renamed track to '{}'", track.name);
                self.track_name_edits
                    .insert(track.id.to_string(), track.name.clone());
                self.refresh_project_after_edit(message);
            }
            Err(error) => self.status = format!("Rename track failed: {error}"),
        }
    }

    fn sync_track_name_edits(&mut self, project: &daw_model::Project) {
        self.track_name_edits.retain(|track_id, _| {
            project
                .tracks
                .iter()
                .any(|track| track.id.to_string() == *track_id)
        });
        for track in &project.tracks {
            self.track_name_edits
                .entry(track.id.to_string())
                .or_insert_with(|| track.name.clone());
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.clip_drag.is_some() {
            return;
        }
        let enter_pressed = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        if enter_pressed && self.project.is_some() {
            self.play_project();
        }
        let space_pressed = ctx.input(|input| input.key_pressed(egui::Key::Space));
        if space_pressed && !ctx.wants_keyboard_input() && self.project.is_some() {
            if self.playback.is_some() {
                self.stop_playback();
            } else {
                self.play_project();
            }
        }
        let delete_pressed = ctx.input(|input| {
            input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace)
        });
        if delete_pressed && !ctx.wants_keyboard_input() && self.selected_clip_id.is_some() {
            self.remove_selected_clip();
        }
    }

    fn use_first_clip(&mut self) {
        let clip = self.project.as_ref().and_then(first_project_clip).cloned();
        if let Some(clip) = clip {
            self.set_clip_edit_fields(&clip);
            "Selected first clip".clone_into(&mut self.status);
        } else if self.project.is_some() {
            "Project has no clips".clone_into(&mut self.status);
        } else {
            "Open a project before selecting a clip".clone_into(&mut self.status);
        }
    }

    fn set_clip_edit_fields(&mut self, clip: &daw_model::Clip) {
        self.edit_clip_id = clip.id.to_string();
        self.edit_clip_start_sample = clip.start_sample.to_string();
        self.edit_clip_duration_samples = clip.duration_samples.to_string();
    }

    fn move_clip(&mut self) {
        let path = PathBuf::from(&self.project_path);
        let start_sample = match parse_u64(&self.edit_clip_start_sample, "clip edit start") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let duration_samples =
            match parse_u64(&self.edit_clip_duration_samples, "clip edit duration") {
                Ok(value) => value,
                Err(error) => {
                    self.status = error;
                    return;
                }
            };
        match daw_model::set_clip_placement(
            &path,
            &daw_model::StableId::from_string(self.edit_clip_id.clone()),
            start_sample,
            duration_samples,
        ) {
            Ok(clip) => {
                let message = format!(
                    "Moved clip {} to {} for {}",
                    clip.id, clip.start_sample, clip.duration_samples
                );
                self.refresh_project_after_edit(message);
            }
            Err(error) => self.status = format!("Move clip failed: {error}"),
        }
    }

    fn remove_clip(&mut self) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::remove_clip(
            &path,
            &daw_model::StableId::from_string(self.edit_clip_id.clone()),
        ) {
            Ok(clip) => {
                self.edit_clip_id.clear();
                if self.selected_clip_id.as_ref() == Some(&clip.id) {
                    self.selected_clip_id = None;
                }
                self.refresh_project_after_edit(format!("Removed clip {}", clip.id));
            }
            Err(error) => self.status = format!("Remove clip failed: {error}"),
        }
    }

    fn remove_selected_clip(&mut self) {
        let Some(clip_id) = self.selected_clip_id.clone() else {
            "No clip selected".clone_into(&mut self.status);
            return;
        };
        let path = PathBuf::from(&self.project_path);
        match daw_model::remove_clip(&path, &clip_id) {
            Ok(clip) => {
                self.selected_clip_id = None;
                self.edit_clip_id.clear();
                self.refresh_project_after_edit(format!("Removed clip {}", clip.id));
            }
            Err(error) => self.status = format!("Remove clip failed: {error}"),
        }
    }

    fn remove_track(&mut self, track_id: &daw_model::StableId) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::remove_track(&path, track_id) {
            Ok(track) => {
                if self.recording_track_id == track.id.to_string() {
                    self.recording_track_id.clear();
                }
                self.selected_clip_id = None;
                self.edit_clip_id.clear();
                self.track_name_edits.remove(&track.id.to_string());
                self.refresh_project_after_edit(format!("Removed track '{}'", track.name));
            }
            Err(error) => self.status = format!("Remove track failed: {error}"),
        }
    }

    fn play_project(&mut self) {
        if self.playback.is_some() {
            self.stop_playback();
        }

        let path = PathBuf::from(&self.project_path);
        let start_sample = match parse_u64(&self.playhead_sample, "playhead") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        match render_project_buffer(&path, 1.0, start_sample, self.metronome_enabled).and_then(
            |buffer| {
                daw_engine::start_buffer_playback(buffer)
                    .map_err(|error| format!("Playback failed: {error}"))
            },
        ) {
            Ok(transport) => {
                let metronome_status = if self.metronome_enabled {
                    " with metronome"
                } else {
                    ""
                };
                self.status = format!(
                    "Playing on '{}'{}",
                    transport.report().device_name,
                    metronome_status
                );
                self.playback = Some(ActivePlayback {
                    transport,
                    start_sample,
                });
            }
            Err(error) => self.status = error,
        }
    }

    fn stop_playback(&mut self) {
        if let Some(mut playback) = self.playback.take() {
            let report = playback.transport.stop();
            let playhead = playback
                .start_sample
                .saturating_add(u64::try_from(report.frames_played).unwrap_or(u64::MAX));
            self.playhead_sample = playhead.to_string();
            self.status = format!(
                "Stopped after {} frames on '{}'",
                report.frames_played, report.device_name
            );
        } else {
            "Nothing is playing".clone_into(&mut self.status);
        }
    }

    fn poll_playback(&mut self) {
        let Some(playback) = self.playback.as_ref() else {
            return;
        };
        let report = playback.transport.report();
        let playhead = playback
            .start_sample
            .saturating_add(u64::try_from(report.frames_played).unwrap_or(u64::MAX));
        self.playhead_sample = playhead.to_string();
        if playback.transport.is_finished() {
            self.stop_playback();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_arrangement(
    ui: &mut egui::Ui,
    project: &daw_model::Project,
    media_objects: &[daw_media::MediaObject],
    waveforms: &[daw_media::WaveformSummary],
    playhead_sample: &str,
    live_recording: Option<&LiveRecordingPreview>,
    timeline_zoom: f32,
    timeline_grid: TimelineGrid,
    selected_clip_id: Option<&daw_model::StableId>,
    active_clip_drag: Option<&ActiveClipDrag>,
    recording_track_id: &str,
    track_name_edits: &mut BTreeMap<String, String>,
) -> Vec<ArrangementAction> {
    let timeline_samples = timeline_sample_span(project, live_recording);
    let playhead = playhead_sample.parse::<u64>().unwrap_or(0);
    let mut actions = Vec::new();

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .scroll_source(ScrollSource {
            drag: false,
            ..ScrollSource::ALL
        })
        .show(ui, |ui| {
            let available_width = ui.available_width().max(860.0);
            let timeline_width = ((available_width - TRACK_HEADER_WIDTH).max(640.0)
                * timeline_zoom.max(0.1))
            .max(640.0);
            if let Some(sample) = render_time_ruler(
                ui,
                timeline_width,
                timeline_samples,
                playhead,
                timeline_grid,
            ) {
                actions.push(ArrangementAction::SetPlayhead(sample));
            }

            for track in &project.tracks {
                actions.extend(render_track_lane(
                    ui,
                    project,
                    track,
                    media_objects,
                    waveforms,
                    timeline_samples,
                    timeline_width,
                    playhead,
                    timeline_grid,
                    live_recording,
                    selected_clip_id,
                    active_clip_drag,
                    recording_track_id,
                    track_name_edits,
                ));
            }

            if active_clip_drag.is_some() {
                let primary_down = ui.input(|input| input.pointer.primary_down());
                if !primary_down {
                    actions.push(ArrangementAction::EndClipDrag);
                }
            }

            if project.tracks.is_empty() {
                ui.add_space(32.0);
                ui.centered_and_justified(|ui| {
                    ui.label("No tracks yet. Add a track, then press Record.");
                });
            }
        });
    actions
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn render_time_ruler(
    ui: &mut egui::Ui,
    timeline_width: f32,
    timeline_samples: u64,
    playhead: u64,
    timeline_grid: TimelineGrid,
) -> Option<u64> {
    let desired = egui::vec2(TRACK_HEADER_WIDTH + timeline_width, 34.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let timeline_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + TRACK_HEADER_WIDTH, rect.top()),
        rect.right_bottom(),
    );
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(28, 30, 34));
    draw_snap_grid(&painter, timeline_rect, timeline_samples, timeline_grid);
    painter.line_segment(
        [timeline_rect.left_bottom(), timeline_rect.right_bottom()],
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(64, 68, 76)),
    );

    for tick in 0..=5 {
        let fraction = tick as f32 / 5.0;
        let x = egui::lerp(timeline_rect.left()..=timeline_rect.right(), fraction);
        painter.line_segment(
            [
                egui::pos2(x, timeline_rect.bottom() - 10.0),
                egui::pos2(x, timeline_rect.bottom()),
            ],
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(93, 98, 108)),
        );
        let sample = ((timeline_samples as f32) * fraction).round() as u64;
        painter.text(
            egui::pos2(x + 4.0, timeline_rect.top() + 7.0),
            egui::Align2::LEFT_TOP,
            format!(
                "{:.1}s",
                sample as f32 / daw_engine::DEFAULT_SAMPLE_RATE as f32
            ),
            egui::FontId::monospace(11.0),
            egui::Color32::from_rgb(188, 192, 200),
        );
    }
    draw_playhead(&painter, timeline_rect, playhead, timeline_samples);
    response
        .interact_pointer_pos()
        .filter(|_| {
            response.clicked() || response.dragged() || response.is_pointer_button_down_on()
        })
        .map(|position| {
            snap_sample(
                sample_from_x(position.x, timeline_rect, timeline_samples),
                timeline_grid.snap,
            )
        })
}

#[allow(clippy::too_many_arguments)]
fn render_track_lane(
    ui: &mut egui::Ui,
    project: &daw_model::Project,
    track: &daw_model::Track,
    media_objects: &[daw_media::MediaObject],
    waveforms: &[daw_media::WaveformSummary],
    timeline_samples: u64,
    timeline_width: f32,
    playhead: u64,
    timeline_grid: TimelineGrid,
    live_recording: Option<&LiveRecordingPreview>,
    selected_clip_id: Option<&daw_model::StableId>,
    active_clip_drag: Option<&ActiveClipDrag>,
    recording_track_id: &str,
    track_name_edits: &mut BTreeMap<String, String>,
) -> Vec<ArrangementAction> {
    let mut actions = Vec::new();
    let desired = egui::vec2(TRACK_HEADER_WIDTH + timeline_width, TRACK_LANE_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let header_rect = egui::Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.left() + TRACK_HEADER_WIDTH, rect.bottom()),
    );
    let lane_rect = egui::Rect::from_min_max(
        egui::pos2(header_rect.right(), rect.top()),
        rect.right_bottom(),
    );

    painter.rect_filled(header_rect, 0.0, egui::Color32::from_rgb(35, 38, 43));
    painter.rect_filled(lane_rect, 0.0, egui::Color32::from_rgb(23, 25, 29));
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(52, 56, 64)),
    );
    if let Some(action) =
        render_track_header_controls(ui, header_rect, track, recording_track_id, track_name_edits)
    {
        actions.push(action);
    }

    draw_lane_grid(&painter, lane_rect);
    draw_snap_grid(&painter, lane_rect, timeline_samples, timeline_grid);
    draw_playhead(&painter, lane_rect, playhead, timeline_samples);

    let mut clip_rects = Vec::new();
    for clip in &track.clips {
        if active_clip_drag.is_some_and(|drag| drag.clip_id == clip.id) {
            clip_rects.push(clip_rect(
                lane_rect,
                clip.start_sample,
                clip.duration_samples,
                timeline_samples,
            ));
            continue;
        }
        let result = render_clip(
            ui,
            project,
            media_objects,
            waveforms,
            lane_rect,
            &track.id,
            clip,
            timeline_samples,
            timeline_grid.snap,
            selected_clip_id,
            active_clip_drag,
        );
        clip_rects.push(result.rect);
        if let Some(action) = result.action {
            actions.push(action);
        }
    }

    if active_clip_drag.is_none() {
        if let Some(sample) = lane_pointer_sample(
            ui,
            lane_rect,
            &clip_rects,
            timeline_samples,
            timeline_grid.snap,
        ) {
            actions.push(ArrangementAction::SetPlayhead(sample));
        }
    }

    if let Some(drag) = active_clip_drag {
        if let Some(action) = update_clip_drag_in_lane(
            ui,
            rect,
            lane_rect,
            track,
            timeline_samples,
            timeline_grid.snap,
        ) {
            actions.push(action);
        }
        if drag.current_track_id == track.id {
            render_active_clip_drag(
                &painter,
                project,
                media_objects,
                waveforms,
                lane_rect,
                drag,
                timeline_samples,
                selected_clip_id,
            );
        }
    }

    if let Some(live_recording) = live_recording.filter(|preview| preview.track_id == track.id) {
        render_live_recording(&painter, lane_rect, live_recording, timeline_samples);
    }

    actions
}

#[allow(clippy::too_many_lines)]
fn render_track_header_controls(
    ui: &mut egui::Ui,
    header_rect: egui::Rect,
    track: &daw_model::Track,
    recording_track_id: &str,
    track_name_edits: &mut BTreeMap<String, String>,
) -> Option<ArrangementAction> {
    let painter = ui.painter_at(header_rect);
    let track_id = track.id.to_string();
    let name = track_name_edits
        .entry(track_id)
        .or_insert_with(|| track.name.clone());
    let name_response = ui.put(
        egui::Rect::from_min_size(
            header_rect.left_top() + egui::vec2(10.0, 8.0),
            egui::vec2(222.0, 22.0),
        ),
        egui::TextEdit::singleline(name).desired_width(222.0),
    );
    painter.text(
        header_rect.left_top() + egui::vec2(12.0, 34.0),
        egui::Align2::LEFT_TOP,
        format!("{}%", track.volume_percent),
        egui::FontId::monospace(12.0),
        egui::Color32::from_rgb(166, 172, 184),
    );

    let mut volume = track.volume_percent;
    let slider_rect = egui::Rect::from_min_size(
        header_rect.left_top() + egui::vec2(70.0, 30.0),
        egui::vec2(118.0, 20.0),
    );
    let volume_response = ui.put(
        slider_rect,
        egui::Slider::new(&mut volume, 0..=200).show_value(false),
    );

    let muted_response = ui.put(
        egui::Rect::from_min_size(
            header_rect.left_top() + egui::vec2(12.0, 58.0),
            egui::vec2(36.0, 24.0),
        ),
        egui::Button::new("M").fill(if track.muted {
            egui::Color32::from_rgb(190, 132, 40)
        } else {
            egui::Color32::from_rgb(57, 61, 69)
        }),
    );
    let solo_response = ui.put(
        egui::Rect::from_min_size(
            header_rect.left_top() + egui::vec2(56.0, 58.0),
            egui::vec2(36.0, 24.0),
        ),
        egui::Button::new("S").fill(if track.solo {
            egui::Color32::from_rgb(55, 132, 92)
        } else {
            egui::Color32::from_rgb(57, 61, 69)
        }),
    );
    let armed = recording_track_id == track.id.to_string();
    let arm_response = ui.put(
        egui::Rect::from_min_size(
            header_rect.left_top() + egui::vec2(104.0, 58.0),
            egui::vec2(46.0, 24.0),
        ),
        egui::Button::new("Rec").fill(if armed {
            egui::Color32::from_rgb(150, 42, 52)
        } else {
            egui::Color32::from_rgb(57, 61, 69)
        }),
    );
    let remove_response = ui.put(
        egui::Rect::from_min_size(
            header_rect.left_top() + egui::vec2(202.0, 58.0),
            egui::vec2(42.0, 24.0),
        ),
        egui::Button::new("Del").fill(egui::Color32::from_rgb(68, 48, 52)),
    );

    let muted = if muted_response.clicked() {
        !track.muted
    } else {
        track.muted
    };
    let solo = if solo_response.clicked() {
        !track.solo
    } else {
        track.solo
    };
    let name_committed = name_response.lost_focus()
        && ui.input(|input| input.key_pressed(egui::Key::Enter) || input.pointer.any_pressed());
    if name_committed && name.trim() != track.name {
        return Some(ArrangementAction::RenameTrack {
            track_id: track.id.clone(),
            name: name.clone(),
        });
    }
    if arm_response.clicked() {
        return Some(ArrangementAction::ArmTrack(track.id.clone()));
    }
    if remove_response.clicked() {
        return Some(ArrangementAction::RemoveTrack(track.id.clone()));
    }
    let volume_changed = volume_response.changed();
    if volume_changed || muted != track.muted || solo != track.solo {
        Some(ArrangementAction::SetTrackControls {
            track_id: track.id.clone(),
            volume_percent: volume,
            muted,
            solo,
        })
    } else {
        None
    }
}

#[allow(clippy::cast_precision_loss)]
fn draw_lane_grid(painter: &egui::Painter, rect: egui::Rect) {
    for tick in 1..5 {
        let fraction = tick as f32 / 5.0;
        let x = egui::lerp(rect.left()..=rect.right(), fraction);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(33, 36, 42)),
        );
    }
}

#[allow(clippy::cast_precision_loss)]
fn draw_snap_grid(
    painter: &egui::Painter,
    rect: egui::Rect,
    timeline_samples: u64,
    timeline_grid: TimelineGrid,
) {
    if let Some(beat_samples) = timeline_grid.beat {
        draw_grid_lines(
            painter,
            rect,
            timeline_samples,
            beat_samples,
            egui::Color32::from_rgb(48, 53, 63),
        );
        if let Some(bar_samples) = timeline_grid.bar {
            draw_grid_lines(
                painter,
                rect,
                timeline_samples,
                bar_samples,
                egui::Color32::from_rgb(60, 66, 78),
            );
        }
        return;
    }
    if let Some(snap_samples) = timeline_grid.snap {
        draw_grid_lines(
            painter,
            rect,
            timeline_samples,
            snap_samples,
            egui::Color32::from_rgb(42, 46, 54),
        );
    }
}

#[allow(clippy::cast_precision_loss)]
fn draw_grid_lines(
    painter: &egui::Painter,
    rect: egui::Rect,
    timeline_samples: u64,
    mut step_samples: u64,
    color: egui::Color32,
) {
    if step_samples == 0 {
        return;
    }
    while timeline_samples / step_samples > 128 {
        step_samples = step_samples.saturating_mul(2);
    }
    let mut sample = step_samples;
    while sample < timeline_samples {
        let fraction = sample as f32 / timeline_samples.max(1) as f32;
        let x = egui::lerp(rect.left()..=rect.right(), fraction);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0_f32, color),
        );
        sample = sample.saturating_add(step_samples);
        if sample == u64::MAX {
            break;
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn draw_playhead(painter: &egui::Painter, rect: egui::Rect, playhead: u64, timeline_samples: u64) {
    let fraction = (playhead as f32 / timeline_samples.max(1) as f32).clamp(0.0, 1.0);
    let x = egui::lerp(rect.left()..=rect.right(), fraction);
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(238, 194, 78)),
    );
}

fn lane_pointer_sample(
    ui: &egui::Ui,
    lane_rect: egui::Rect,
    clip_rects: &[egui::Rect],
    timeline_samples: u64,
    snap_grid_samples: Option<u64>,
) -> Option<u64> {
    let pointer = ui.input(|input| input.pointer.clone());
    if !pointer.primary_down() {
        return None;
    }
    let position = pointer.latest_pos()?;
    if !lane_rect.contains(position) || clip_rects.iter().any(|rect| rect.contains(position)) {
        return None;
    }
    Some(snap_sample(
        sample_from_x(position.x, lane_rect, timeline_samples),
        snap_grid_samples,
    ))
}

fn update_clip_drag_in_lane(
    ui: &egui::Ui,
    row_rect: egui::Rect,
    lane_rect: egui::Rect,
    track: &daw_model::Track,
    timeline_samples: u64,
    snap_grid_samples: Option<u64>,
) -> Option<ArrangementAction> {
    let pointer = ui.input(|input| input.pointer.clone());
    if !pointer.primary_down() {
        return None;
    }
    let position = pointer.latest_pos()?;
    if position.y < row_rect.top() || position.y > row_rect.bottom() {
        return None;
    }
    Some(ArrangementAction::UpdateClipDrag {
        track_id: track.id.clone(),
        pointer_x: position.x,
        lane_width: lane_rect.width(),
        timeline_samples,
        snap_grid_samples,
    })
}

#[allow(clippy::cast_precision_loss)]
fn clip_rect(
    lane_rect: egui::Rect,
    start_sample: u64,
    duration_samples: u64,
    timeline_samples: u64,
) -> egui::Rect {
    let start_fraction = start_sample as f32 / timeline_samples.max(1) as f32;
    let end_sample = start_sample.saturating_add(duration_samples);
    let end_fraction = end_sample as f32 / timeline_samples.max(1) as f32;
    let x1 = egui::lerp(
        lane_rect.left()..=lane_rect.right(),
        start_fraction.clamp(0.0, 1.0),
    );
    let x2 = egui::lerp(
        lane_rect.left()..=lane_rect.right(),
        end_fraction.clamp(0.0, 1.0),
    );
    egui::Rect::from_min_max(
        egui::pos2(x1, lane_rect.top() + 12.0),
        egui::pos2(x2.max(x1 + 8.0), lane_rect.bottom() - 12.0),
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_clip_body(
    painter: &egui::Painter,
    project: &daw_model::Project,
    media_objects: &[daw_media::MediaObject],
    waveforms: &[daw_media::WaveformSummary],
    clip: &daw_model::Clip,
    rect: egui::Rect,
    selected: bool,
    highlighted: bool,
) {
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(49, 111, 120));
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(87, 180, 190)),
        egui::StrokeKind::Inside,
    );

    let waveform = clip_waveform(project, waveforms, clip);
    if let Some(waveform) = waveform {
        draw_waveform(
            painter,
            rect.shrink2(egui::vec2(8.0, 14.0)),
            &waveform.peaks,
        );
    } else {
        let label = clip_media_label(project, media_objects, clip);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(218, 238, 240),
        );
    }

    if highlighted || selected {
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(
                2.0_f32,
                if selected {
                    egui::Color32::from_rgb(248, 231, 126)
                } else {
                    egui::Color32::from_rgb(235, 226, 150)
                },
            ),
            egui::StrokeKind::Inside,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_active_clip_drag(
    painter: &egui::Painter,
    project: &daw_model::Project,
    media_objects: &[daw_media::MediaObject],
    waveforms: &[daw_media::WaveformSummary],
    lane_rect: egui::Rect,
    drag: &ActiveClipDrag,
    timeline_samples: u64,
    selected_clip_id: Option<&daw_model::StableId>,
) {
    let Some(clip) = selected_clip(project, Some(&drag.clip_id)) else {
        return;
    };
    let rect = clip_rect(
        lane_rect,
        drag.current_start_sample,
        drag.duration_samples,
        timeline_samples,
    );
    draw_clip_body(
        painter,
        project,
        media_objects,
        waveforms,
        clip,
        rect,
        selected_clip_id == Some(&drag.clip_id),
        true,
    );
}

#[allow(
    clippy::cast_precision_loss,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
fn render_clip(
    ui: &mut egui::Ui,
    project: &daw_model::Project,
    media_objects: &[daw_media::MediaObject],
    waveforms: &[daw_media::WaveformSummary],
    lane_rect: egui::Rect,
    track_id: &daw_model::StableId,
    clip: &daw_model::Clip,
    timeline_samples: u64,
    snap_grid_samples: Option<u64>,
    selected_clip_id: Option<&daw_model::StableId>,
    active_clip_drag: Option<&ActiveClipDrag>,
) -> ClipRenderResult {
    let painter = ui.painter_at(lane_rect);
    let base_clip_rect = clip_rect(
        lane_rect,
        clip.start_sample,
        clip.duration_samples,
        timeline_samples,
    );
    let response = ui.interact(
        base_clip_rect,
        egui::Id::new(("clip", clip.id.to_string())),
        egui::Sense::click_and_drag(),
    );
    let selected = selected_clip_id == Some(&clip.id);
    let this_clip_drag = active_clip_drag.filter(|drag| drag.clip_id == clip.id);
    let draw_rect = if let Some(drag) = this_clip_drag {
        clip_rect(
            lane_rect,
            drag.current_start_sample,
            drag.duration_samples,
            timeline_samples,
        )
    } else {
        base_clip_rect
    };

    draw_clip_body(
        &painter,
        project,
        media_objects,
        waveforms,
        clip,
        draw_rect,
        selected,
        this_clip_drag.is_some() || response.hovered(),
    );

    if let Some(drag) = this_clip_drag {
        let mouse_pointer = ui.input(|input| input.pointer.clone());
        if mouse_pointer.primary_down() {
            if let Some(position) = mouse_pointer.latest_pos() {
                return ClipRenderResult {
                    rect: base_clip_rect,
                    action: Some(ArrangementAction::UpdateClipDrag {
                        track_id: track_id.clone(),
                        pointer_x: position.x,
                        lane_width: lane_rect.width(),
                        timeline_samples,
                        snap_grid_samples,
                    }),
                };
            }
        } else {
            return ClipRenderResult {
                rect: clip_rect(
                    lane_rect,
                    drag.current_start_sample,
                    drag.duration_samples,
                    timeline_samples,
                ),
                action: Some(ArrangementAction::EndClipDrag),
            };
        }
    }

    if active_clip_drag.is_none() && response.is_pointer_button_down_on() {
        if let Some(position) = response.interact_pointer_pos() {
            return ClipRenderResult {
                rect: base_clip_rect,
                action: Some(ArrangementAction::BeginClipDrag {
                    clip_id: clip.id.clone(),
                    track_id: track_id.clone(),
                    start_sample: clip.start_sample,
                    duration_samples: clip.duration_samples,
                    pointer_x: position.x,
                }),
            };
        }
    }

    if response.clicked() {
        return ClipRenderResult {
            rect: base_clip_rect,
            action: Some(ArrangementAction::SelectClip(clip.id.clone())),
        };
    }

    ClipRenderResult {
        rect: base_clip_rect,
        action: None,
    }
}

#[allow(clippy::cast_precision_loss)]
fn render_live_recording(
    painter: &egui::Painter,
    lane_rect: egui::Rect,
    preview: &LiveRecordingPreview,
    timeline_samples: u64,
) {
    let start_fraction = preview.start_sample as f32 / timeline_samples.max(1) as f32;
    let end_sample = preview
        .start_sample
        .saturating_add(preview.duration_samples);
    let end_fraction = end_sample as f32 / timeline_samples.max(1) as f32;
    let x1 = egui::lerp(
        lane_rect.left()..=lane_rect.right(),
        start_fraction.clamp(0.0, 1.0),
    );
    let x2 = egui::lerp(
        lane_rect.left()..=lane_rect.right(),
        end_fraction.clamp(0.0, 1.0),
    );
    let rect = egui::Rect::from_min_max(
        egui::pos2(x1, lane_rect.top() + 12.0),
        egui::pos2(x2.max(x1 + 8.0), lane_rect.bottom() - 12.0),
    );
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(118, 43, 52));
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(230, 98, 112)),
        egui::StrokeKind::Inside,
    );
    draw_waveform(painter, rect.shrink2(egui::vec2(8.0, 14.0)), &preview.peaks);
}

#[allow(clippy::cast_precision_loss)]
fn draw_waveform(painter: &egui::Painter, rect: egui::Rect, peaks: &[daw_media::WaveformPeak]) {
    if peaks.is_empty() {
        return;
    }
    let center_y = rect.center().y;
    for (index, peak) in peaks.iter().enumerate() {
        let fraction = index as f32 / peaks.len().max(1) as f32;
        let x = egui::lerp(rect.left()..=rect.right(), fraction);
        let min_y = center_y - peak.max.clamp(-1.0, 1.0) * rect.height() * 0.5;
        let max_y = center_y - peak.min.clamp(-1.0, 1.0) * rect.height() * 0.5;
        painter.line_segment(
            [egui::pos2(x, min_y), egui::pos2(x, max_y)],
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(214, 244, 245)),
        );
    }
}

fn clip_waveform<'a>(
    project: &daw_model::Project,
    waveforms: &'a [daw_media::WaveformSummary],
    clip: &daw_model::Clip,
) -> Option<&'a daw_media::WaveformSummary> {
    let hash = project
        .media
        .iter()
        .find(|media| media.id == clip.media_id)?
        .content_hash
        .as_deref()?;
    waveforms.iter().find(|waveform| waveform.hash == hash)
}

fn clip_media_label(
    project: &daw_model::Project,
    media_objects: &[daw_media::MediaObject],
    clip: &daw_model::Clip,
) -> String {
    let Some(media) = project.media.iter().find(|media| media.id == clip.media_id) else {
        return "missing media".to_owned();
    };
    let Some(hash) = media.content_hash.as_deref() else {
        return "unlinked media".to_owned();
    };
    media_objects
        .iter()
        .find(|object| object.hash == hash)
        .and_then(|object| object.original_path.rsplit('/').next())
        .unwrap_or("audio clip")
        .to_owned()
}

fn timeline_sample_span(
    project: &daw_model::Project,
    live_recording: Option<&LiveRecordingPreview>,
) -> u64 {
    let project_span = project
        .tracks
        .iter()
        .flat_map(|track| &track.clips)
        .map(|clip| clip.start_sample.saturating_add(clip.duration_samples))
        .max()
        .unwrap_or(MIN_TIMELINE_SAMPLES);
    let live_span = live_recording.map_or(MIN_TIMELINE_SAMPLES, |preview| {
        preview
            .start_sample
            .saturating_add(preview.duration_samples)
    });
    project_span.max(live_span).max(MIN_TIMELINE_SAMPLES)
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn pixels_to_samples(delta_pixels: f32, timeline_width: f32, timeline_samples: u64) -> i64 {
    if timeline_width <= 0.0 {
        return 0;
    }
    ((delta_pixels / timeline_width) * timeline_samples as f32).round() as i64
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn sample_from_x(x: f32, timeline_rect: egui::Rect, timeline_samples: u64) -> u64 {
    let fraction =
        ((x - timeline_rect.left()) / timeline_rect.width().max(1.0_f32)).clamp(0.0, 1.0);
    (fraction * timeline_samples as f32).round() as u64
}

fn apply_sample_delta(sample: u64, delta: i64) -> u64 {
    if delta >= 0 {
        sample.saturating_add(delta.unsigned_abs())
    } else {
        sample.saturating_sub(delta.unsigned_abs())
    }
}

fn snap_grid_samples_from_ms(milliseconds: u32) -> u64 {
    (u64::from(milliseconds) * u64::from(daw_engine::DEFAULT_SAMPLE_RATE)) / 1_000
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn samples_per_beat(tempo_bpm: u16) -> u64 {
    ((f64::from(daw_engine::DEFAULT_SAMPLE_RATE) * 60.0) / f64::from(tempo_bpm.max(1))).round()
        as u64
}

fn snap_sample(sample: u64, grid_samples: Option<u64>) -> u64 {
    let Some(grid_samples) = grid_samples.filter(|samples| *samples > 0) else {
        return sample;
    };
    let remainder = sample % grid_samples;
    let lower = sample - remainder;
    if remainder >= grid_samples - remainder {
        lower.saturating_add(grid_samples)
    } else {
        lower
    }
}

fn waveform_peaks_from_buffer(
    buffer: &daw_engine::AudioBuffer,
    target_points: usize,
) -> Vec<daw_media::WaveformPeak> {
    let frames = buffer.frames();
    if frames == 0 {
        return Vec::new();
    }
    let frames_per_peak = frames.div_ceil(target_points.max(1)).max(1);
    let channels = usize::from(buffer.channels);
    let mut peaks = Vec::new();
    for start_frame in (0..frames).step_by(frames_per_peak) {
        let end_frame = (start_frame + frames_per_peak).min(frames);
        let mut min = 1.0_f32;
        let mut max = -1.0_f32;
        for frame in start_frame..end_frame {
            for channel in 0..channels {
                let sample = buffer.samples[frame * channels + channel].clamp(-1.0, 1.0);
                min = min.min(sample);
                max = max.max(sample);
            }
        }
        peaks.push(daw_media::WaveformPeak { min, max });
    }
    peaks
}

fn first_project_clip(project: &daw_model::Project) -> Option<&daw_model::Clip> {
    project.tracks.iter().flat_map(|track| &track.clips).next()
}

fn selected_clip<'a>(
    project: &'a daw_model::Project,
    clip_id: Option<&daw_model::StableId>,
) -> Option<&'a daw_model::Clip> {
    let clip_id = clip_id?;
    project
        .tracks
        .iter()
        .flat_map(|track| &track.clips)
        .find(|clip| &clip.id == clip_id)
}

fn render_project_buffer(
    project_path: &Path,
    minimum_duration: f32,
    start_sample: u64,
    include_metronome: bool,
) -> Result<daw_engine::AudioBuffer, String> {
    let project = daw_model::load_project(project_path)
        .map_err(|error| format!("Project is invalid: {error}"))?;
    let media_objects = daw_media::list_media(project_path)
        .map_err(|error| format!("Failed to list media: {error}"))?;
    let mut total_frames = duration_to_frames(minimum_duration, daw_engine::DEFAULT_SAMPLE_RATE)?;
    for track in &project.tracks {
        for clip in &track.clips {
            let clip_end = clip.start_sample.saturating_add(clip.duration_samples);
            if clip_end > start_sample {
                let relative_end = usize::try_from(clip_end - start_sample)
                    .map_err(|_| "Clip timeline position is too large".to_owned())?;
                total_frames = total_frames.max(relative_end);
            }
        }
    }

    let mut output = daw_engine::AudioBuffer {
        sample_rate: daw_engine::DEFAULT_SAMPLE_RATE,
        channels: daw_engine::DEFAULT_CHANNELS,
        samples: vec![0.0; total_frames * usize::from(daw_engine::DEFAULT_CHANNELS)],
    };
    let solo_active = project.tracks.iter().any(|track| track.solo);

    for track in &project.tracks {
        if track.muted || (solo_active && !track.solo) {
            continue;
        }
        for clip in &track.clips {
            let clip_end = clip.start_sample.saturating_add(clip.duration_samples);
            if clip_end <= start_sample {
                continue;
            }
            mix_clip_from_project(
                project_path,
                &project,
                &media_objects,
                &mut output,
                track,
                clip,
                start_sample,
            )?;
        }
    }

    if include_metronome {
        mix_metronome_into_buffer(&mut output, project.tempo_bpm)?;
    }

    Ok(output)
}

fn mix_metronome_into_buffer(
    output: &mut daw_engine::AudioBuffer,
    tempo_bpm: u16,
) -> Result<(), String> {
    let frames = output.frames();
    if frames == 0 {
        return Ok(());
    }
    let bar_frames = samples_per_beat(tempo_bpm).saturating_mul(BEATS_PER_BAR);
    let bars = frames
        .div_ceil(usize::try_from(bar_frames.max(1)).unwrap_or(usize::MAX))
        .max(1);
    let bars = u32::try_from(bars).map_err(|_| "Metronome render is too long".to_owned())?;
    let metronome = daw_engine::render_metronome(
        tempo_bpm,
        u16::try_from(BEATS_PER_BAR).unwrap_or(4),
        bars,
        output.sample_rate,
        output.channels,
    )
    .map_err(|error| format!("Metronome render failed: {error}"))?;
    daw_engine::mix_clip(output, &metronome, 0, 100, false);
    Ok(())
}

fn mix_clip_from_project(
    project_path: &Path,
    project: &daw_model::Project,
    media_objects: &[daw_media::MediaObject],
    output: &mut daw_engine::AudioBuffer,
    track: &daw_model::Track,
    clip: &daw_model::Clip,
    render_start_sample: u64,
) -> Result<(), String> {
    let media = project
        .media
        .iter()
        .find(|media| media.id == clip.media_id)
        .ok_or_else(|| {
            format!(
                "Clip {} references unknown media {}",
                clip.id, clip.media_id
            )
        })?;
    let hash = media
        .content_hash
        .as_deref()
        .ok_or_else(|| format!("Media {} has no content hash", media.id))?;
    let object = media_objects
        .iter()
        .find(|object| object.hash == hash)
        .ok_or_else(|| format!("Media hash {hash} is not imported"))?;
    let path =
        daw_media::media_object_path(project_path, &object.hash, object.extension.as_deref());
    let decoded = daw_engine::read_wav(&path)
        .map_err(|error| format!("Failed to read media {hash}: {error}"))?;
    if decoded.sample_rate != output.sample_rate {
        return Err(format!(
            "Media {hash} is {} Hz; expected {} Hz",
            decoded.sample_rate, output.sample_rate
        ));
    }
    let decoded = daw_engine::convert_channels(&decoded, output.channels);
    let clip_end = clip.start_sample.saturating_add(clip.duration_samples);
    let source_start = if render_start_sample > clip.start_sample {
        usize::try_from(render_start_sample - clip.start_sample)
            .map_err(|_| "Clip source start is too large".to_owned())?
    } else {
        0
    };
    let destination_start = if clip.start_sample >= render_start_sample {
        usize::try_from(clip.start_sample - render_start_sample)
            .map_err(|_| "Clip start is too large".to_owned())?
    } else {
        0
    };
    let remaining_clip_frames =
        usize::try_from(clip_end - render_start_sample.max(clip.start_sample))
            .map_err(|_| "Clip duration is too large".to_owned())?;
    let limited = slice_buffer_frames(&decoded, source_start, remaining_clip_frames);
    daw_engine::mix_clip(
        output,
        &limited,
        destination_start,
        track.volume_percent,
        track.muted,
    );
    Ok(())
}

fn slice_buffer_frames(
    buffer: &daw_engine::AudioBuffer,
    start_frame: usize,
    frames: usize,
) -> daw_engine::AudioBuffer {
    daw_engine::slice_frames(buffer, start_frame, frames)
}

fn parse_u64(value: &str, label: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("Invalid {label}: {error}"))
}

fn insert_recorded_audio(
    project_path: &Path,
    track_id: &daw_model::StableId,
    start_sample: u64,
    recorded: &daw_engine::RecordedAudio,
) -> Result<RecordingInsertReport, String> {
    let output_path = recording_output_path(project_path, recorded.report.frames_recorded);
    daw_engine::write_wav(&output_path, &recorded.buffer)
        .map_err(|error| format!("recording write failed: {error}"))?;
    let object = daw_media::import_media(project_path, &output_path)
        .map_err(|error| format!("recording import failed: {error}"))?;
    let media =
        daw_model::add_media_reference(project_path, &object.hash, Some(object.original_path))
            .map_err(|error| format!("recording registration failed: {error}"))?;
    let duration_samples = u64::try_from(recorded.buffer.frames())
        .map_err(|_| "recording is too long for the project model".to_owned())?;
    let clip = daw_model::add_clip(
        project_path,
        track_id,
        &media.id,
        start_sample,
        duration_samples,
    )
    .map_err(|error| format!("recorded clip insert failed: {error}"))?;

    Ok(RecordingInsertReport {
        message: format!(
            "Recorded {} frames from '{}' into clip {}",
            recorded.report.frames_recorded, recorded.report.device_name, clip.id
        ),
        media_hash: object.hash,
    })
}

fn recording_output_path(project_path: &Path, frames_recorded: usize) -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    project_path
        .join("recordings")
        .join(format!("recording-{timestamp}-{frames_recorded}.wav"))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn duration_to_frames(duration: f32, sample_rate: u32) -> Result<usize, String> {
    if duration <= 0.0 {
        return Err("Duration must be greater than zero".to_owned());
    }
    Ok((f64::from(duration) * f64::from(sample_rate)).round() as usize)
}

#[cfg(test)]
mod tests {
    use super::{
        mix_metronome_into_buffer, samples_per_beat, snap_grid_samples_from_ms, snap_sample,
    };

    #[test]
    fn converts_snap_grid_milliseconds_to_samples() {
        assert_eq!(snap_grid_samples_from_ms(250), 12_000);
        assert_eq!(snap_grid_samples_from_ms(1_000), 48_000);
    }

    #[test]
    fn converts_tempo_to_beat_samples() {
        assert_eq!(samples_per_beat(120), 24_000);
        assert_eq!(samples_per_beat(96), 30_000);
        assert_eq!(samples_per_beat(60), 48_000);
    }

    #[test]
    fn snaps_samples_to_nearest_grid_line() {
        assert_eq!(snap_sample(11_999, Some(12_000)), 12_000);
        assert_eq!(snap_sample(6_000, Some(12_000)), 12_000);
        assert_eq!(snap_sample(5_999, Some(12_000)), 0);
        assert_eq!(snap_sample(18_001, Some(12_000)), 24_000);
        assert_eq!(snap_sample(18_001, None), 18_001);
    }

    #[test]
    fn mixes_metronome_into_existing_buffer() {
        let mut buffer = daw_engine::AudioBuffer {
            sample_rate: daw_engine::DEFAULT_SAMPLE_RATE,
            channels: daw_engine::DEFAULT_CHANNELS,
            samples: vec![0.0; 48_000 * usize::from(daw_engine::DEFAULT_CHANNELS)],
        };

        mix_metronome_into_buffer(&mut buffer, 120).expect("mix metronome");

        assert!(buffer.samples.iter().any(|sample| sample.abs() > 0.0));
    }
}
