//! Native desktop UI shell for the DAW.

use eframe::egui;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
};

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
    playback_rx: Option<Receiver<String>>,
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
            playback_rx: None,
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
                    self.play_test_tone();
                }
                if ui.button("Stop").clicked() {
                    "Stop requested; short test playback will end automatically"
                        .clone_into(&mut self.status);
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

    fn play_test_tone(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.playback_rx = Some(rx);
        "Starting playback".clone_into(&mut self.status);
        thread::spawn(move || {
            let message = match daw_engine::play_test_tone(1.0) {
                Ok(report) => format!(
                    "Played on '{}' with {} stream errors",
                    report.device_name, report.stream_errors
                ),
                Err(error) => format!("Playback failed: {error}"),
            };
            let _ = tx.send(message);
        });
    }

    fn poll_playback(&mut self) {
        if let Some(rx) = &self.playback_rx {
            if let Ok(message) = rx.try_recv() {
                self.status = message;
                self.playback_rx = None;
            }
        }
    }
}
