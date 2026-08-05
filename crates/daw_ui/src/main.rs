//! Native desktop UI shell for the DAW.

use eframe::egui;
use std::path::{Path, PathBuf};

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
    snapshot_message: String,
    status: String,
    project: Option<daw_model::Project>,
    media: Vec<daw_media::MediaObject>,
    history: Vec<daw_model::HistoryItem>,
    transport: Option<daw_engine::PlaybackTransport>,
    recording: Option<ActiveRecording>,
}

struct ActiveRecording {
    transport: daw_engine::RecordingTransport,
    track_id: daw_model::StableId,
    start_sample: u64,
}

impl Default for DawApp {
    fn default() -> Self {
        Self {
            project_path: "/private/tmp/daw-ui-project".to_owned(),
            new_project_name: "UI Project".to_owned(),
            new_track_name: "Audio".to_owned(),
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
            snapshot_message: "UI snapshot".to_owned(),
            status: "No project loaded".to_owned(),
            project: None,
            media: Vec::new(),
            history: Vec::new(),
            transport: None,
            recording: None,
        }
    }
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll_playback();
        self.render_transport(ctx);
        self.render_inspector(ctx);
        self.render_project(ctx);
    }
}

impl DawApp {
    fn render_transport(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("transport").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Project");
                ui.text_edit_singleline(&mut self.project_path);
                if ui.button("Create").clicked() {
                    self.create_project();
                }
                if ui.button("Open").clicked() {
                    self.reload_project();
                }
                if ui.button("Validate").clicked() {
                    self.validate_project();
                }
                if ui.button("Play").clicked() {
                    self.play_project();
                }
                if ui.button("Stop").clicked() {
                    self.stop_playback();
                }
            });
        });
    }

    fn render_inspector(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("inspector")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Project");
                self.render_project_edit_section(ui);
                self.render_media_clip_section(ui);
                self.render_clip_edit_section(ui);
                self.render_snapshot_section(ui);
                self.render_recording_section(ui);
                self.render_mixer_section(ui);
                ui.separator();
                ui.label(&self.status);
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

    fn render_recording_section(&mut self, ui: &mut egui::Ui) {
        ui.label("Recording track id");
        ui.text_edit_singleline(&mut self.recording_track_id);
        ui.horizontal(|ui| {
            ui.label("Start");
            ui.text_edit_singleline(&mut self.recording_start_sample);
        });
        ui.horizontal(|ui| {
            if ui.button("Use First Track").clicked() {
                self.use_first_recording_track();
            }
            if ui.button("Start Record").clicked() {
                self.start_recording();
            }
            if ui.button("Stop Record").clicked() {
                self.stop_recording();
            }
        });
        ui.separator();
    }

    fn render_mixer_section(&mut self, ui: &mut egui::Ui) {
        ui.label("Mixer track id");
        ui.text_edit_singleline(&mut self.mixer_track_id);
        ui.label("Volume percent");
        ui.text_edit_singleline(&mut self.mixer_volume_percent);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.mixer_muted, "Muted");
            ui.checkbox(&mut self.mixer_solo, "Solo");
        });
        ui.horizontal(|ui| {
            if ui.button("Use First Track").clicked() {
                self.use_first_mixer_track();
            }
            if ui.button("Set Controls").clicked() {
                self.set_track_controls();
            }
        });
    }

    fn render_project(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(project) = &self.project {
                ui.heading(&project.name);
                ui.horizontal(|ui| {
                    ui.label(format!("tracks: {}", project.tracks.len()));
                    ui.label(format!("media: {}", project.media.len()));
                });
                ui.separator();
                ui.columns(3, |columns| {
                    render_tracks_column(&mut columns[0], project);
                    render_media_column(&mut columns[1], project, &self.media);
                    render_history_column(&mut columns[2], &self.history);
                });
            } else {
                ui.heading("No project loaded");
                ui.label("Create or open a project to begin.");
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

    fn use_first_mixer_track(&mut self) {
        let track = self
            .project
            .as_ref()
            .and_then(|project| project.tracks.first().cloned());
        if let Some(track) = track {
            self.set_mixer_fields_from_track(&track);
            "Selected first track controls".clone_into(&mut self.status);
        } else if self.project.is_some() {
            "Project has no tracks".clone_into(&mut self.status);
        } else {
            "Open a project before selecting track controls".clone_into(&mut self.status);
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
        let start_sample = match parse_u64(&self.recording_start_sample, "recording start") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
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
                    self.status = report;
                    self.reload_project();
                }
                Err(error) => self.status = format!("Record insert failed: {error}"),
            },
            Err(error) => self.status = format!("Record stop failed: {error}"),
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
                self.reload_project();
            }
            Err(error) => self.status = format!("Remove clip failed: {error}"),
        }
    }

    fn set_track_controls(&mut self) {
        let path = PathBuf::from(&self.project_path);
        let volume_percent = match parse_u16(&self.mixer_volume_percent, "track volume") {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };

        match daw_model::set_track_controls(
            &path,
            &daw_model::StableId::from_string(self.mixer_track_id.clone()),
            volume_percent,
            self.mixer_muted,
            self.mixer_solo,
        ) {
            Ok(track) => {
                self.status = format!(
                    "Set '{}' controls: volume={} muted={} solo={}",
                    track.name, track.volume_percent, track.muted, track.solo
                );
                self.reload_project();
            }
            Err(error) => self.status = format!("Set controls failed: {error}"),
        }
    }

    fn play_project(&mut self) {
        if self.transport.is_some() {
            self.stop_playback();
        }

        let path = PathBuf::from(&self.project_path);
        match render_project_buffer(&path, 1.0).and_then(|buffer| {
            daw_engine::start_buffer_playback(buffer)
                .map_err(|error| format!("Playback failed: {error}"))
        }) {
            Ok(transport) => {
                self.status = format!("Playing on '{}'", transport.report().device_name);
                self.transport = Some(transport);
            }
            Err(error) => self.status = error,
        }
    }

    fn stop_playback(&mut self) {
        if let Some(mut transport) = self.transport.take() {
            let report = transport.stop();
            self.status = format!(
                "Stopped after {} frames on '{}'",
                report.frames_played, report.device_name
            );
        } else {
            "Nothing is playing".clone_into(&mut self.status);
        }
    }

    fn poll_playback(&mut self) {
        if self
            .transport
            .as_ref()
            .is_some_and(daw_engine::PlaybackTransport::is_finished)
        {
            self.stop_playback();
        }
    }
}

fn render_tracks_column(ui: &mut egui::Ui, project: &daw_model::Project) {
    ui.heading("Tracks");
    for track in &project.tracks {
        ui.group(|ui| {
            ui.label(&track.name);
            ui.monospace(track.id.to_string());
            ui.label(format!(
                "volume: {} muted: {} solo: {}",
                track.volume_percent, track.muted, track.solo
            ));
            ui.label(format!("clips: {}", track.clips.len()));
            for clip in &track.clips {
                ui.label(format!(
                    "clip {} at {} for {}",
                    clip.id, clip.start_sample, clip.duration_samples
                ));
            }
        });
    }
}

fn render_media_column(
    ui: &mut egui::Ui,
    project: &daw_model::Project,
    media_objects: &[daw_media::MediaObject],
) {
    ui.heading("Media");
    for media_ref in &project.media {
        ui.group(|ui| {
            ui.label("project ref");
            ui.monospace(media_ref.id.to_string());
            if let Some(hash) = &media_ref.content_hash {
                ui.label(hash);
            }
        });
    }
    for media in media_objects {
        ui.group(|ui| {
            ui.label("store object");
            ui.monospace(&media.hash);
            ui.label(format!("{} bytes", media.byte_size));
        });
    }
}

fn render_history_column(ui: &mut egui::Ui, history: &[daw_model::HistoryItem]) {
    ui.heading("History");
    for item in history {
        ui.group(|ui| {
            ui.monospace(item.id.to_string());
            ui.label(&item.summary);
        });
    }
}

fn first_project_clip(project: &daw_model::Project) -> Option<&daw_model::Clip> {
    project.tracks.iter().flat_map(|track| &track.clips).next()
}

fn render_project_buffer(
    project_path: &Path,
    minimum_duration: f32,
) -> Result<daw_engine::AudioBuffer, String> {
    let project = daw_model::load_project(project_path)
        .map_err(|error| format!("Project is invalid: {error}"))?;
    let media_objects = daw_media::list_media(project_path)
        .map_err(|error| format!("Failed to list media: {error}"))?;
    let mut total_frames = duration_to_frames(minimum_duration, daw_engine::DEFAULT_SAMPLE_RATE)?;
    for track in &project.tracks {
        for clip in &track.clips {
            let clip_end = usize::try_from(clip.start_sample.saturating_add(clip.duration_samples))
                .map_err(|_| "Clip timeline position is too large".to_owned())?;
            total_frames = total_frames.max(clip_end);
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
            mix_clip_from_project(
                project_path,
                &project,
                &media_objects,
                &mut output,
                track,
                clip,
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
    let limited = limit_buffer_frames(
        &decoded,
        usize::try_from(clip.duration_samples)
            .map_err(|_| "Clip duration is too large".to_owned())?,
    );
    daw_engine::mix_clip(
        output,
        &limited,
        usize::try_from(clip.start_sample).map_err(|_| "Clip start is too large".to_owned())?,
        track.volume_percent,
        track.muted,
    );
    Ok(())
}

fn limit_buffer_frames(buffer: &daw_engine::AudioBuffer, frames: usize) -> daw_engine::AudioBuffer {
    let channels = usize::from(buffer.channels);
    let sample_count = buffer.samples.len().min(frames.saturating_mul(channels));
    daw_engine::AudioBuffer {
        sample_rate: buffer.sample_rate,
        channels: buffer.channels,
        samples: buffer.samples[..sample_count].to_vec(),
    }
}

fn parse_u64(value: &str, label: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("Invalid {label}: {error}"))
}

fn parse_u16(value: &str, label: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|error| format!("Invalid {label}: {error}"))
}

fn insert_recorded_audio(
    project_path: &Path,
    track_id: &daw_model::StableId,
    start_sample: u64,
    recorded: &daw_engine::RecordedAudio,
) -> Result<String, String> {
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

    Ok(format!(
        "Recorded {} frames from '{}' into clip {}",
        recorded.report.frames_recorded, recorded.report.device_name, clip.id
    ))
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
