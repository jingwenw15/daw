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
    snapshot_message: String,
    status: String,
    project: Option<daw_model::Project>,
    media: Vec<daw_media::MediaObject>,
    history: Vec<daw_model::HistoryItem>,
    transport: Option<daw_engine::PlaybackTransport>,
}

impl Default for DawApp {
    fn default() -> Self {
        Self {
            project_path: "/private/tmp/daw-ui-project".to_owned(),
            new_project_name: "UI Project".to_owned(),
            new_track_name: "Audio".to_owned(),
            snapshot_message: "UI snapshot".to_owned(),
            status: "No project loaded".to_owned(),
            project: None,
            media: Vec::new(),
            history: Vec::new(),
            transport: None,
        }
    }
}

impl eframe::App for DawApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll_playback();

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

        egui::SidePanel::left("inspector")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Project");
                ui.label("New project name");
                ui.text_edit_singleline(&mut self.new_project_name);
                ui.separator();
                ui.label("New track");
                ui.text_edit_singleline(&mut self.new_track_name);
                if ui.button("Add Track").clicked() {
                    self.add_track();
                }
                ui.separator();
                ui.label("Snapshot message");
                ui.text_edit_singleline(&mut self.snapshot_message);
                if ui.button("Create Snapshot").clicked() {
                    self.create_snapshot();
                }
                ui.separator();
                ui.label(&self.status);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(project) = &self.project {
                ui.heading(&project.name);
                ui.horizontal(|ui| {
                    ui.label(format!("tracks: {}", project.tracks.len()));
                    ui.label(format!("media: {}", project.media.len()));
                });
                ui.separator();
                ui.columns(3, |columns| {
                    columns[0].heading("Tracks");
                    for track in &project.tracks {
                        columns[0].group(|ui| {
                            ui.label(&track.name);
                            ui.monospace(track.id.to_string());
                            ui.label(format!("clips: {}", track.clips.len()));
                        });
                    }

                    columns[1].heading("Media");
                    for media in &self.media {
                        columns[1].group(|ui| {
                            ui.monospace(&media.hash);
                            ui.label(format!("{} bytes", media.byte_size));
                        });
                    }

                    columns[2].heading("History");
                    for item in &self.history {
                        columns[2].group(|ui| {
                            ui.monospace(item.id.to_string());
                            ui.label(&item.summary);
                        });
                    }
                });
            } else {
                ui.heading("No project loaded");
                ui.label("Create or open a project to begin.");
            }
        });
    }
}

impl DawApp {
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
                self.reload_project();
            }
            Err(error) => self.status = format!("Add track failed: {error}"),
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
    if decoded.sample_rate != output.sample_rate || decoded.channels != output.channels {
        return Err(format!(
            "Media {hash} is {} Hz/{} channels; expected {} Hz/{} channels",
            decoded.sample_rate, decoded.channels, output.sample_rate, output.channels
        ));
    }
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

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn duration_to_frames(duration: f32, sample_rate: u32) -> Result<usize, String> {
    if duration <= 0.0 {
        return Err("Duration must be greater than zero".to_owned());
    }
    Ok((f64::from(duration) * f64::from(sample_rate)).round() as usize)
}
