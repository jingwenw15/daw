//! Native desktop UI shell for the DAW.

use eframe::egui::{self, scroll_area::ScrollSource};
use std::path::{Path, PathBuf};

const TRACK_HEADER_WIDTH: f32 = 260.0;
const TRACK_LANE_HEIGHT: f32 = 92.0;
const MIN_TIMELINE_SAMPLES: u64 = 240_000;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1120.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native("DAW", options, Box::new(|_| Ok(Box::<DawApp>::default())))
}

struct DawApp {
    project_path: String,
    new_project_name: String,
    new_track_name: String,
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
    selected_clip_id: Option<daw_model::StableId>,
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
    track_id: daw_model::StableId,
    start_sample: u64,
}

struct ActivePlayback {
    transport: daw_engine::PlaybackTransport,
    start_sample: u64,
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
    start_sample: u64,
    duration_samples: u64,
}

#[derive(Clone, Debug)]
enum ArrangementAction {
    MoveClip(ClipMoveRequest),
    SetPlayhead(u64),
    SelectClip(daw_model::StableId),
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
            selected_clip_id: None,
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
        if self.recording.is_some() || self.playback.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
        self.render_transport(ctx);
        self.render_utilities(ctx);
        self.render_project(ctx);
    }
}

impl DawApp {
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
                ui.horizontal(|ui| {
                    ui.heading(&project.name);
                    if self.recording.is_some() {
                        ui.colored_label(egui::Color32::from_rgb(220, 72, 82), "recording");
                    }
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
                    self.selected_clip_id.as_ref(),
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

    fn create_project(&mut self) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::init_project(&path, &self.new_project_name) {
            Ok(project) => {
                self.project = Some(project);
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
                self.project = Some(project);
                self.status = format!("Opened {}", path.display());
                self.reload_auxiliary();
            }
            Err(error) => self.status = format!("Open failed: {error}"),
        }
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
                self.status = format!("Added track '{}'", track.name);
                self.clip_track_id = track.id.to_string();
                self.mixer_track_id = track.id.to_string();
                self.recording_track_id = track.id.to_string();
                self.reload_project();
            }
            Err(error) => self.status = format!("Add track failed: {error}"),
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
                self.status = format!("Imported {} bytes as {}", object.byte_size, object.hash);
                self.reload_project();
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
                self.status = format!("Added clip {}", clip.id);
                self.set_clip_edit_fields(&clip);
                self.reload_project();
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
                self.status = format!("Recording from '{}'", transport.report().device_name);
                self.recording = Some(ActiveRecording {
                    transport,
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
                    self.status = report.message;
                    self.reload_project();
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

    fn commit_clip_move(&mut self, request: &ClipMoveRequest) {
        let path = PathBuf::from(&self.project_path);
        match daw_model::set_clip_placement(
            &path,
            &request.clip_id,
            request.start_sample,
            request.duration_samples,
        ) {
            Ok(clip) => {
                self.status = format!("Moved clip {} to {}", clip.id, clip.start_sample);
                self.set_clip_edit_fields(&clip);
                self.reload_project();
            }
            Err(error) => self.status = format!("Move clip failed: {error}"),
        }
    }

    fn apply_arrangement_action(&mut self, action: &ArrangementAction) {
        match action {
            ArrangementAction::MoveClip(request) => self.commit_clip_move(request),
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
                self.status = format!(
                    "Set '{}' controls: volume={} muted={} solo={}",
                    track.name, track.volume_percent, track.muted, track.solo
                );
                self.set_mixer_fields_from_track(&track);
                self.reload_project();
            }
            Err(error) => self.status = format!("Set controls failed: {error}"),
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
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
                self.status = format!(
                    "Moved clip {} to {} for {}",
                    clip.id, clip.start_sample, clip.duration_samples
                );
                self.reload_project();
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
                self.status = format!("Removed clip {}", clip.id);
                self.edit_clip_id.clear();
                if self.selected_clip_id.as_ref() == Some(&clip.id) {
                    self.selected_clip_id = None;
                }
                self.reload_project();
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
                self.status = format!("Removed clip {}", clip.id);
                self.selected_clip_id = None;
                self.edit_clip_id.clear();
                self.reload_project();
            }
            Err(error) => self.status = format!("Remove clip failed: {error}"),
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
        match render_project_buffer(&path, 1.0, start_sample).and_then(|buffer| {
            daw_engine::start_buffer_playback(buffer)
                .map_err(|error| format!("Playback failed: {error}"))
        }) {
            Ok(transport) => {
                self.status = format!("Playing on '{}'", transport.report().device_name);
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
    selected_clip_id: Option<&daw_model::StableId>,
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
            if let Some(sample) = render_time_ruler(ui, timeline_width, timeline_samples, playhead)
            {
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
                    live_recording,
                    selected_clip_id,
                ));
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
) -> Option<u64> {
    let desired = egui::vec2(TRACK_HEADER_WIDTH + timeline_width, 34.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
    let painter = ui.painter_at(rect);
    let timeline_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + TRACK_HEADER_WIDTH, rect.top()),
        rect.right_bottom(),
    );
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(28, 30, 34));
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
        .filter(|_| response.clicked())
        .map(|position| sample_from_x(position.x, timeline_rect, timeline_samples))
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
    live_recording: Option<&LiveRecordingPreview>,
    selected_clip_id: Option<&daw_model::StableId>,
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
    if let Some(action) = render_track_header_controls(ui, header_rect, track) {
        actions.push(action);
    }

    draw_lane_grid(&painter, lane_rect);
    draw_playhead(&painter, lane_rect, playhead, timeline_samples);
    let lane_response = ui.interact(
        lane_rect,
        egui::Id::new(("lane", track.id.to_string())),
        egui::Sense::click(),
    );
    if let Some(position) = lane_response
        .interact_pointer_pos()
        .filter(|_| lane_response.clicked())
    {
        actions.push(ArrangementAction::SetPlayhead(sample_from_x(
            position.x,
            lane_rect,
            timeline_samples,
        )));
    }

    for clip in &track.clips {
        if let Some(action) = render_clip(
            ui,
            project,
            media_objects,
            waveforms,
            lane_rect,
            clip,
            timeline_samples,
            selected_clip_id,
        ) {
            actions.push(action);
        }
    }

    if let Some(live_recording) = live_recording.filter(|preview| preview.track_id == track.id) {
        render_live_recording(&painter, lane_rect, live_recording, timeline_samples);
    }

    actions
}

fn render_track_header_controls(
    ui: &mut egui::Ui,
    header_rect: egui::Rect,
    track: &daw_model::Track,
) -> Option<ArrangementAction> {
    let painter = ui.painter_at(header_rect);
    painter.text(
        header_rect.left_top() + egui::vec2(12.0, 10.0),
        egui::Align2::LEFT_TOP,
        &track.name,
        egui::FontId::proportional(16.0),
        egui::Color32::from_rgb(235, 236, 240),
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
fn draw_playhead(painter: &egui::Painter, rect: egui::Rect, playhead: u64, timeline_samples: u64) {
    let fraction = (playhead as f32 / timeline_samples.max(1) as f32).clamp(0.0, 1.0);
    let x = egui::lerp(rect.left()..=rect.right(), fraction);
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(238, 194, 78)),
    );
}

#[allow(clippy::cast_precision_loss, clippy::too_many_arguments)]
fn render_clip(
    ui: &mut egui::Ui,
    project: &daw_model::Project,
    media_objects: &[daw_media::MediaObject],
    waveforms: &[daw_media::WaveformSummary],
    lane_rect: egui::Rect,
    clip: &daw_model::Clip,
    timeline_samples: u64,
    selected_clip_id: Option<&daw_model::StableId>,
) -> Option<ArrangementAction> {
    let painter = ui.painter_at(lane_rect);
    let start_fraction = clip.start_sample as f32 / timeline_samples.max(1) as f32;
    let end_sample = clip.start_sample.saturating_add(clip.duration_samples);
    let end_fraction = end_sample as f32 / timeline_samples.max(1) as f32;
    let x1 = egui::lerp(
        lane_rect.left()..=lane_rect.right(),
        start_fraction.clamp(0.0, 1.0),
    );
    let x2 = egui::lerp(
        lane_rect.left()..=lane_rect.right(),
        end_fraction.clamp(0.0, 1.0),
    );
    let clip_rect = egui::Rect::from_min_max(
        egui::pos2(x1, lane_rect.top() + 12.0),
        egui::pos2(x2.max(x1 + 8.0), lane_rect.bottom() - 12.0),
    );
    let response = ui.interact(
        clip_rect,
        egui::Id::new(("clip", clip.id.to_string())),
        egui::Sense::click_and_drag(),
    );
    let selected = selected_clip_id == Some(&clip.id);
    let draw_rect = if response.dragged() {
        clip_rect.translate(egui::vec2(response.drag_delta().x, 0.0))
    } else {
        clip_rect
    };

    painter.rect_filled(draw_rect, 4.0, egui::Color32::from_rgb(49, 111, 120));
    painter.rect_stroke(
        draw_rect,
        4.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(87, 180, 190)),
        egui::StrokeKind::Inside,
    );

    let waveform = clip_waveform(project, waveforms, clip);
    if let Some(waveform) = waveform {
        draw_waveform(
            &painter,
            draw_rect.shrink2(egui::vec2(8.0, 14.0)),
            &waveform.peaks,
        );
    } else {
        let label = clip_media_label(project, media_objects, clip);
        painter.text(
            draw_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(218, 238, 240),
        );
    }
    if response.dragged() || response.hovered() {
        painter.rect_stroke(
            draw_rect,
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
    if selected && !response.dragged() && !response.hovered() {
        painter.rect_stroke(
            draw_rect,
            4.0,
            egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(248, 231, 126)),
            egui::StrokeKind::Inside,
        );
    }

    if response.clicked() {
        return Some(ArrangementAction::SelectClip(clip.id.clone()));
    }

    if response.drag_stopped() {
        let delta_pixels = response
            .total_drag_delta()
            .map_or(response.drag_delta().x, |delta| delta.x);
        let delta_samples = pixels_to_samples(delta_pixels, lane_rect.width(), timeline_samples);
        let start_sample = apply_sample_delta(clip.start_sample, delta_samples);
        return Some(ArrangementAction::MoveClip(ClipMoveRequest {
            clip_id: clip.id.clone(),
            start_sample,
            duration_samples: clip.duration_samples,
        }));
    }

    None
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

    Ok(output)
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
