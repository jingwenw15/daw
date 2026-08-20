//! Command-line entrypoint for the DAW.

use std::{env, path::PathBuf, process::ExitCode};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let mut args = args.into_iter();

    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("daw {VERSION}");
            Ok(())
        }
        Some("--help" | "-h") | None => {
            print_help();
            Ok(())
        }
        Some("init") => {
            let path = required_arg(&mut args, "path")?;
            let name = args.next().unwrap_or_else(|| default_project_name(&path));
            no_extra_args(args)?;
            let project = daw_model::init_project(path.as_ref(), &name)
                .map_err(|error| format!("failed to initialize project: {error}"))?;
            println!(
                "initialized project '{}' at {}",
                project.name,
                std::path::Path::new(&path).display()
            );
            Ok(())
        }
        Some("validate") => {
            let path = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            daw_model::load_project(path.as_ref())
                .map_err(|error| format!("project is invalid: {error}"))?;
            println!("project is valid");
            Ok(())
        }
        Some("inspect") => {
            let path = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            print_project(
                &daw_model::load_project(path.as_ref())
                    .map_err(|error| format!("failed to inspect project: {error}"))?,
            )
        }
        Some("project") => run_project(args),
        Some("track") => run_track(args),
        Some("clip") => run_clip(args),
        Some("snapshot") => run_snapshot(args),
        Some("branch") => run_branch(args),
        Some("vcs") => run_vcs(args),
        Some("media") => run_media(args),
        Some("render-test-tone") => run_render_test_tone(args),
        Some("render-metronome") => run_render_metronome(args),
        Some("render-project") => run_render_project(args),
        Some("play-project") => run_play_project(args),
        Some("play-metronome") => run_play_metronome(args),
        Some("play-test-tone") => run_play_test_tone(args),
        Some("record-snippet") => run_record_snippet(args),
        Some("history") => {
            let path = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            let items = daw_model::history(path.as_ref())
                .map_err(|error| format!("failed to load history: {error}"))?;
            for item in items {
                println!("{} {}", item.id, item.summary);
            }
            Ok(())
        }
        Some("undo") => run_undo(args),
        Some("redo") => run_redo(args),
        Some("checkout-snapshot") => run_checkout_snapshot(args),
        Some("diff") => run_diff(args),
        Some("merge") => run_merge(args),
        Some("replay") => {
            let path = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            let project = daw_model::replay_project(path.as_ref())
                .map_err(|error| format!("failed to replay project: {error}"))?;
            print_project(&project)
        }
        Some(command) => Err(format!(
            "unknown command: {command}\nrun `daw --help` for usage"
        )),
    }
}

fn run_undo(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let path = required_arg(&mut args, "path")?;
    no_extra_args(args)?;
    let project =
        daw_model::undo_project(path.as_ref()).map_err(|error| format!("undo failed: {error}"))?;
    println!(
        "undid latest command; project has {} tracks and {} media references",
        project.tracks.len(),
        project.media.len()
    );
    Ok(())
}

fn run_redo(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let path = required_arg(&mut args, "path")?;
    no_extra_args(args)?;
    let project =
        daw_model::redo_project(path.as_ref()).map_err(|error| format!("redo failed: {error}"))?;
    println!(
        "redid latest command; project has {} tracks and {} media references",
        project.tracks.len(),
        project.media.len()
    );
    Ok(())
}

fn run_record_snippet(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let project_path = required_arg(&mut args, "path")?;
    let track_id = required_arg(&mut args, "track-id")?;
    let duration = optional_f32(args.next(), 1.0, "duration-seconds")?;
    let start_sample = optional_u64(args.next(), 0, "start-sample")?;
    no_extra_args(args)?;

    let recorded = daw_engine::record_input(duration)
        .map_err(|error| format!("failed to record snippet: {error}"))?;
    let output_path = recording_output_path(&project_path, recorded.report.frames_recorded);
    daw_engine::write_wav(&output_path, &recorded.buffer)
        .map_err(|error| format!("failed to write recording: {error}"))?;
    let object = daw_media::import_media(project_path.as_ref(), &output_path)
        .map_err(|error| format!("failed to import recording: {error}"))?;
    let media = daw_model::add_media_reference(
        project_path.as_ref(),
        &object.hash,
        Some(object.original_path.clone()),
    )
    .map_err(|error| format!("failed to register recording media: {error}"))?;
    let duration_samples = u64::try_from(recorded.buffer.frames())
        .map_err(|_| "recording is too long for the project model".to_owned())?;
    let clip = daw_model::add_clip(
        project_path.as_ref(),
        &daw_model::StableId::from_string(track_id),
        &media.id,
        start_sample,
        duration_samples,
    )
    .map_err(|error| format!("failed to add recorded clip: {error}"))?;

    println!(
        "recorded {} frames from '{}' at {} Hz, {} channels",
        recorded.report.frames_recorded,
        recorded.report.device_name,
        recorded.report.sample_rate,
        recorded.report.channels
    );
    println!("stream errors: {}", recorded.report.stream_errors);
    println!("recording file: {}", output_path.display());
    println!("media id: {}", media.id);
    println!("clip id: {}", clip.id);
    Ok(())
}

fn run_play_test_tone(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let duration = optional_f32(args.next(), 1.0, "duration-seconds")?;
    no_extra_args(args)?;
    let report = daw_engine::play_test_tone(duration)
        .map_err(|error| format!("failed to play test tone: {error}"))?;
    println!("played test tone on '{}'", report.device_name);
    println!(
        "{} frames at {} Hz, {} channels, stream errors: {}",
        report.frames_played, report.sample_rate, report.channels, report.stream_errors
    );
    Ok(())
}

fn run_play_metronome(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let tempo_bpm = required_u16(&mut args, "tempo-bpm")?;
    let bars = optional_u32(args.next(), 4, "bars")?;
    let beats_per_bar = optional_u16(args.next(), 4, "beats-per-bar")?;
    no_extra_args(args)?;
    let buffer = daw_engine::render_metronome(
        tempo_bpm,
        beats_per_bar,
        bars,
        daw_engine::DEFAULT_SAMPLE_RATE,
        daw_engine::DEFAULT_CHANNELS,
    )
    .map_err(|error| format!("failed to render metronome: {error}"))?;
    let hold_seconds = frames_to_seconds(buffer.frames(), buffer.sample_rate) + 0.10;
    let report = daw_engine::play_buffer(buffer, hold_seconds)
        .map_err(|error| format!("failed to play metronome: {error}"))?;
    println!(
        "played metronome: {tempo_bpm} BPM, {beats_per_bar}/4, {bars} bars on '{}'",
        report.device_name
    );
    Ok(())
}

fn run_play_project(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let project_path = required_arg(&mut args, "path")?;
    let duration = optional_f32(args.next(), 1.0, "minimum-duration-seconds")?;
    let start_sample = optional_u64(args.next(), 0, "start-sample")?;
    no_extra_args(args)?;
    let buffer = render_project_buffer(&project_path, duration, start_sample)?;
    let hold_seconds = frames_to_seconds(buffer.frames(), buffer.sample_rate) + 0.10;
    let report = daw_engine::play_buffer(buffer, hold_seconds)
        .map_err(|error| format!("failed to play project: {error}"))?;
    println!("played project on '{}'", report.device_name);
    println!(
        "{} frames at {} Hz, {} channels, stream errors: {}",
        report.frames_played, report.sample_rate, report.channels, report.stream_errors
    );
    Ok(())
}

fn run_render_test_tone(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let output = required_arg(&mut args, "output")?;
    let duration = optional_f32(args.next(), 1.0, "duration-seconds")?;
    no_extra_args(args)?;
    let buffer = daw_engine::render_sine(
        440.0,
        duration,
        0.25,
        daw_engine::DEFAULT_SAMPLE_RATE,
        daw_engine::DEFAULT_CHANNELS,
    )
    .map_err(|error| format!("failed to render test tone: {error}"))?;
    daw_engine::write_wav(output.as_ref(), &buffer)
        .map_err(|error| format!("failed to write test tone: {error}"))?;
    println!(
        "rendered test tone: {} frames at {} Hz -> {}",
        buffer.frames(),
        buffer.sample_rate,
        output
    );
    Ok(())
}

fn run_render_metronome(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let output = required_arg(&mut args, "output")?;
    let tempo_bpm = required_u16(&mut args, "tempo-bpm")?;
    let bars = optional_u32(args.next(), 4, "bars")?;
    let beats_per_bar = optional_u16(args.next(), 4, "beats-per-bar")?;
    no_extra_args(args)?;
    let buffer = daw_engine::render_metronome(
        tempo_bpm,
        beats_per_bar,
        bars,
        daw_engine::DEFAULT_SAMPLE_RATE,
        daw_engine::DEFAULT_CHANNELS,
    )
    .map_err(|error| format!("failed to render metronome: {error}"))?;
    daw_engine::write_wav(output.as_ref(), &buffer)
        .map_err(|error| format!("failed to write metronome: {error}"))?;
    println!("rendered metronome: {tempo_bpm} BPM, {beats_per_bar}/4, {bars} bars -> {output}");
    Ok(())
}

fn run_render_project(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let project_path = required_arg(&mut args, "path")?;
    let output = required_arg(&mut args, "output")?;
    let duration = optional_f32(args.next(), 1.0, "duration-seconds")?;
    let start_sample = optional_u64(args.next(), 0, "start-sample")?;
    no_extra_args(args)?;
    let buffer = render_project_buffer(&project_path, duration, start_sample)?;
    daw_engine::write_wav(output.as_ref(), &buffer)
        .map_err(|error| format!("failed to write project render: {error}"))?;
    println!(
        "rendered project: {} frames at {} Hz -> {}",
        buffer.frames(),
        buffer.sample_rate,
        output
    );
    Ok(())
}

fn run_media(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("import") => {
            let project = required_arg(&mut args, "path")?;
            let source = required_arg(&mut args, "source")?;
            no_extra_args(args)?;
            let object = daw_media::import_media(project.as_ref(), source.as_ref())
                .map_err(|error| format!("failed to import media: {error}"))?;
            let media = daw_model::add_media_reference(
                project.as_ref(),
                &object.hash,
                Some(object.original_path.clone()),
            )
            .map_err(|error| format!("failed to register media in project: {error}"))?;
            println!("imported media {}", object.hash);
            println!("media id: {}", media.id);
            println!("bytes: {}", object.byte_size);
            println!("source: {}", object.original_path);
            Ok(())
        }
        Some("list") => {
            let project = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            let objects = daw_media::list_media(project.as_ref())
                .map_err(|error| format!("failed to list media: {error}"))?;
            if objects.is_empty() {
                println!("media: none");
            } else {
                for object in objects {
                    println!(
                        "{} {} bytes {}",
                        object.hash, object.byte_size, object.original_path
                    );
                }
            }
            Ok(())
        }
        Some("verify") => {
            let project = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            let results = daw_media::verify_media(project.as_ref())
                .map_err(|error| format!("failed to verify media: {error}"))?;
            if results.is_empty() {
                println!("media: none");
            } else {
                for result in results {
                    let status = if result.ok { "ok" } else { "missing" };
                    println!("{} {} {}", status, result.hash, result.message);
                }
            }
            Ok(())
        }
        Some("relink") => {
            let project = required_arg(&mut args, "path")?;
            let hash = required_arg(&mut args, "hash")?;
            let replacement = required_arg(&mut args, "replacement")?;
            no_extra_args(args)?;
            let object = daw_media::relink_media(project.as_ref(), &hash, replacement.as_ref())
                .map_err(|error| format!("failed to relink media: {error}"))?;
            println!("relinked media {}", object.hash);
            println!("source: {}", object.original_path);
            Ok(())
        }
        Some("waveform") => {
            let project = required_arg(&mut args, "path")?;
            let hash = required_arg(&mut args, "hash")?;
            let points = optional_usize(args.next(), daw_media::DEFAULT_WAVEFORM_POINTS, "points")?;
            no_extra_args(args)?;
            let waveform = daw_media::generate_waveform(project.as_ref(), &hash, points)
                .map_err(|error| format!("failed to generate waveform: {error}"))?;
            println!(
                "waveform {}: {} peaks, {} frames/peak",
                waveform.hash,
                waveform.peaks.len(),
                waveform.frames_per_peak
            );
            Ok(())
        }
        Some("waveforms") => {
            let project = required_arg(&mut args, "path")?;
            let points = optional_usize(args.next(), daw_media::DEFAULT_WAVEFORM_POINTS, "points")?;
            no_extra_args(args)?;
            let waveforms = daw_media::generate_waveforms(project.as_ref(), points)
                .map_err(|error| format!("failed to generate waveforms: {error}"))?;
            println!("generated {} waveform caches", waveforms.len());
            for waveform in waveforms {
                println!(
                    "{} {} peaks {} frames/peak",
                    waveform.hash,
                    waveform.peaks.len(),
                    waveform.frames_per_peak
                );
            }
            Ok(())
        }
        Some(command) => Err(format!(
            "unknown media command: {command}\nrun `daw --help` for usage"
        )),
        None => Err("missing media command\nrun `daw --help` for usage".to_owned()),
    }
}

fn run_clip(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("add") => {
            let project = required_arg(&mut args, "path")?;
            let track_id = required_arg(&mut args, "track-id")?;
            let media_id = required_arg(&mut args, "media-id")?;
            let start_sample = required_u64(&mut args, "start-sample")?;
            let duration_samples = required_u64(&mut args, "duration-samples")?;
            no_extra_args(args)?;
            let clip = daw_model::add_clip(
                project.as_ref(),
                &daw_model::StableId::from_string(track_id),
                &daw_model::StableId::from_string(media_id),
                start_sample,
                duration_samples,
            )
            .map_err(|error| format!("failed to add clip: {error}"))?;
            println!("added clip {}", clip.id);
            Ok(())
        }
        Some("move") => {
            let project = required_arg(&mut args, "path")?;
            let clip_id = required_arg(&mut args, "clip-id")?;
            let start_sample = required_u64(&mut args, "start-sample")?;
            let duration_samples = required_u64(&mut args, "duration-samples")?;
            no_extra_args(args)?;
            let clip = daw_model::set_clip_placement(
                project.as_ref(),
                &daw_model::StableId::from_string(clip_id),
                start_sample,
                duration_samples,
            )
            .map_err(|error| format!("failed to move clip: {error}"))?;
            println!(
                "moved clip {} to {} for {}",
                clip.id, clip.start_sample, clip.duration_samples
            );
            Ok(())
        }
        Some("split") => {
            let project = required_arg(&mut args, "path")?;
            let clip_id = required_arg(&mut args, "clip-id")?;
            let split_sample = required_u64(&mut args, "split-sample")?;
            no_extra_args(args)?;
            let (left, right) = daw_model::split_clip(
                project.as_ref(),
                &daw_model::StableId::from_string(clip_id),
                split_sample,
            )
            .map_err(|error| format!("failed to split clip: {error}"))?;
            println!(
                "split clip into {} ({} samples) and {} ({} samples)",
                left.id, left.duration_samples, right.id, right.duration_samples
            );
            Ok(())
        }
        Some("duplicate") => {
            let project = required_arg(&mut args, "path")?;
            let clip_id = required_arg(&mut args, "clip-id")?;
            let start_sample = required_u64(&mut args, "start-sample")?;
            let track_id = args.next().map(daw_model::StableId::from_string);
            no_extra_args(args)?;
            let clip = daw_model::duplicate_clip(
                project.as_ref(),
                &daw_model::StableId::from_string(clip_id),
                track_id.as_ref(),
                start_sample,
            )
            .map_err(|error| format!("failed to duplicate clip: {error}"))?;
            println!(
                "duplicated clip {} at {} for {}",
                clip.id, clip.start_sample, clip.duration_samples
            );
            Ok(())
        }
        Some("fade") => run_clip_fade(args),
        Some("remove") => {
            let project = required_arg(&mut args, "path")?;
            let clip_id = required_arg(&mut args, "clip-id")?;
            no_extra_args(args)?;
            let clip = daw_model::remove_clip(
                project.as_ref(),
                &daw_model::StableId::from_string(clip_id),
            )
            .map_err(|error| format!("failed to remove clip: {error}"))?;
            println!("removed clip {}", clip.id);
            Ok(())
        }
        Some(command) => Err(format!(
            "unknown clip command: {command}\nrun `daw --help` for usage"
        )),
        None => Err("missing clip command\nrun `daw --help` for usage".to_owned()),
    }
}

fn run_clip_fade(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let project = required_arg(&mut args, "path")?;
    let clip_id = required_arg(&mut args, "clip-id")?;
    let fade_in_samples = required_u64(&mut args, "fade-in-samples")?;
    let fade_out_samples = required_u64(&mut args, "fade-out-samples")?;
    no_extra_args(args)?;
    let clip = daw_model::set_clip_fades(
        project.as_ref(),
        &daw_model::StableId::from_string(clip_id),
        fade_in_samples,
        fade_out_samples,
    )
    .map_err(|error| format!("failed to set clip fades: {error}"))?;
    println!(
        "set clip {} fades in={} out={}",
        clip.id, clip.fade_in_samples, clip.fade_out_samples
    );
    Ok(())
}

fn run_project(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("tempo") => {
            let path = required_arg(&mut args, "path")?;
            let tempo_bpm = required_u16(&mut args, "tempo-bpm")?;
            no_extra_args(args)?;
            let project = daw_model::set_project_tempo(path.as_ref(), tempo_bpm)
                .map_err(|error| format!("failed to set project tempo: {error}"))?;
            println!("set project tempo to {} BPM", project.tempo_bpm);
            Ok(())
        }
        Some(command) => Err(format!(
            "unknown project command: {command}\nrun `daw --help` for usage"
        )),
        None => Err("missing project command\nrun `daw --help` for usage".to_owned()),
    }
}

fn run_vcs(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("init-git") => {
            let path = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            daw_vcs::init_git(path.as_ref())
                .map_err(|error| format!("failed to initialize Git: {error}"))?;
            println!("initialized Git repository");
            if daw_vcs::git_lfs_available(path.as_ref()) {
                println!("Git LFS available; media file patterns written to .gitattributes");
            } else {
                println!("Git LFS not available; large media will remain regular Git files unless configured later");
            }
            Ok(())
        }
        Some("remote") => run_vcs_remote(args),
        Some("status") => {
            let path = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            let status = daw_vcs::status(path.as_ref())
                .map_err(|error| format!("failed to read Git status: {error}"))?;
            if !status.repository_exists {
                println!("Git repository: not initialized");
            } else if status.lines.is_empty() {
                println!("Git repository: clean");
            } else {
                println!("Git repository: changes");
                for line in status.lines {
                    println!("{line}");
                }
            }
            Ok(())
        }
        Some("commit") => {
            let path = required_arg(&mut args, "path")?;
            let message = required_arg(&mut args, "message")?;
            no_extra_args(args)?;
            daw_vcs::commit(path.as_ref(), &message)
                .map_err(|error| format!("failed to commit project: {error}"))?;
            println!("committed project changes");
            Ok(())
        }
        Some("push") => {
            let path = required_arg(&mut args, "path")?;
            let remote = args.next().unwrap_or_else(|| "origin".to_owned());
            let branch = args.next().map_or_else(|| current_git_branch(&path), Ok)?;
            no_extra_args(args)?;
            daw_vcs::push(path.as_ref(), &remote, &branch)
                .map_err(|error| format!("failed to push project: {error}"))?;
            println!("pushed {branch} to {remote}");
            Ok(())
        }
        Some("pull") => {
            let path = required_arg(&mut args, "path")?;
            let remote = args.next().unwrap_or_else(|| "origin".to_owned());
            let branch = args.next().map_or_else(|| current_git_branch(&path), Ok)?;
            no_extra_args(args)?;
            daw_vcs::pull(path.as_ref(), &remote, &branch)
                .map_err(|error| format!("failed to pull project: {error}"))?;
            println!("pulled {branch} from {remote}");
            Ok(())
        }
        Some("lfs-status") => {
            let path = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            if daw_vcs::git_lfs_available(path.as_ref()) {
                println!("Git LFS: available");
            } else {
                println!("Git LFS: not available");
            }
            Ok(())
        }
        Some(command) => Err(format!(
            "unknown vcs command: {command}\nrun `daw --help` for usage"
        )),
        None => Err("missing vcs command\nrun `daw --help` for usage".to_owned()),
    }
}

fn current_git_branch(path: &str) -> Result<String, String> {
    daw_vcs::current_branch(path.as_ref())
        .map_err(|error| format!("failed to determine current Git branch: {error}"))
}

fn run_vcs_remote(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("add") => {
            let path = required_arg(&mut args, "path")?;
            let name = required_arg(&mut args, "name")?;
            let url = required_arg(&mut args, "url")?;
            no_extra_args(args)?;
            let remote = daw_vcs::add_remote(path.as_ref(), &name, &url)
                .map_err(|error| format!("failed to configure remote: {error}"))?;
            println!("configured remote '{}' -> {}", remote.name, remote.url);
            Ok(())
        }
        Some(command) => Err(format!(
            "unknown vcs remote command: {command}\nrun `daw --help` for usage"
        )),
        None => Err("missing vcs remote command\nrun `daw --help` for usage".to_owned()),
    }
}

fn run_branch(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("create") => {
            let path = required_arg(&mut args, "path")?;
            let name = required_arg(&mut args, "name")?;
            no_extra_args(args)?;
            let branch = daw_model::create_branch(path.as_ref(), &name)
                .map_err(|error| format!("failed to create branch: {error}"))?;
            println!("created branch '{}'", branch.name);
            Ok(())
        }
        Some("list") => {
            let path = required_arg(&mut args, "path")?;
            no_extra_args(args)?;
            for branch in daw_model::list_branches(path.as_ref())
                .map_err(|error| format!("failed to list branches: {error}"))?
            {
                println!("{branch}");
            }
            Ok(())
        }
        Some("switch") => {
            let path = required_arg(&mut args, "path")?;
            let name = required_arg(&mut args, "name")?;
            no_extra_args(args)?;
            daw_model::switch_branch(path.as_ref(), &name)
                .map_err(|error| format!("failed to switch branch: {error}"))?;
            println!("switched to branch '{name}'");
            Ok(())
        }
        Some(command) => Err(format!(
            "unknown branch command: {command}\nrun `daw --help` for usage"
        )),
        None => Err("missing branch command\nrun `daw --help` for usage".to_owned()),
    }
}

fn run_track(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("add") => {
            let path = required_arg(&mut args, "path")?;
            let name = required_arg(&mut args, "name")?;
            no_extra_args(args)?;
            let track = daw_model::add_track(path.as_ref(), &name)
                .map_err(|error| format!("failed to add track: {error}"))?;
            println!("added track '{}' ({})", track.name, track.id);
            Ok(())
        }
        Some("controls") => {
            let path = required_arg(&mut args, "path")?;
            let track_id = required_arg(&mut args, "track-id")?;
            let volume_percent = required_u16(&mut args, "volume-percent")?;
            let muted = required_bool(&mut args, "muted")?;
            let solo = required_bool(&mut args, "solo")?;
            no_extra_args(args)?;
            let track = daw_model::set_track_controls(
                path.as_ref(),
                &daw_model::StableId::from_string(track_id),
                volume_percent,
                muted,
                solo,
            )
            .map_err(|error| format!("failed to set track controls: {error}"))?;
            println!(
                "set track '{}' controls: volume={} muted={} solo={}",
                track.name, track.volume_percent, track.muted, track.solo
            );
            Ok(())
        }
        Some(command) => Err(format!(
            "unknown track command: {command}\nrun `daw --help` for usage"
        )),
        None => Err("missing track command\nrun `daw --help` for usage".to_owned()),
    }
}

fn run_snapshot(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("create") => {
            let path = required_arg(&mut args, "path")?;
            let message = args.next().unwrap_or_else(|| "manual snapshot".to_owned());
            no_extra_args(args)?;
            let snapshot = daw_model::create_snapshot(path.as_ref(), &message)
                .map_err(|error| format!("failed to create snapshot: {error}"))?;
            println!("created snapshot '{}' ({})", snapshot.message, snapshot.id);
            Ok(())
        }
        Some(command) => Err(format!(
            "unknown snapshot command: {command}\nrun `daw --help` for usage"
        )),
        None => Err("missing snapshot command\nrun `daw --help` for usage".to_owned()),
    }
}

fn run_diff(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let path = required_arg(&mut args, "path")?;
    let left = required_arg(&mut args, "left-ref")?;
    let right = required_arg(&mut args, "right-ref")?;
    no_extra_args(args)?;
    let diff = daw_model::diff(path.as_ref(), &left, &right)
        .map_err(|error| format!("failed to diff project refs: {error}"))?;
    print_named_list("added tracks", &diff.added_tracks);
    print_named_list("removed tracks", &diff.removed_tracks);
    print_named_list("changed tracks", &diff.changed_tracks);
    Ok(())
}

fn run_merge(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let path = required_arg(&mut args, "path")?;
    let source_branch = required_arg(&mut args, "source-branch")?;
    no_extra_args(args)?;
    let report = daw_model::merge_branch(path.as_ref(), &source_branch)
        .map_err(|error| format!("failed to merge branch: {error}"))?;
    println!("merged branch '{}'", report.source_branch);
    print_named_list("added tracks", &report.added_tracks);
    Ok(())
}

fn run_checkout_snapshot(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let path = required_arg(&mut args, "path")?;
    let snapshot_id = required_arg(&mut args, "snapshot-id")?;
    no_extra_args(args)?;
    let project = daw_model::checkout_snapshot(
        path.as_ref(),
        &daw_model::StableId::from_string(snapshot_id),
    )
    .map_err(|error| format!("failed to checkout snapshot: {error}"))?;
    println!("checked out snapshot into project '{}'", project.name);
    Ok(())
}

fn print_project(project: &daw_model::Project) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(project)
            .map_err(|error| format!("failed to format project: {error}"))?
    );
    Ok(())
}

fn print_named_list(label: &str, values: &[String]) {
    if values.is_empty() {
        println!("{label}: none");
    } else {
        println!("{label}:");
        for value in values {
            println!("  {value}");
        }
    }
}

fn render_project_buffer(
    project_path: &str,
    minimum_duration: f32,
    start_sample: u64,
) -> Result<daw_engine::AudioBuffer, String> {
    let project = daw_model::load_project(project_path.as_ref())
        .map_err(|error| format!("project is invalid: {error}"))?;
    let media_objects = daw_media::list_media(project_path.as_ref())
        .map_err(|error| format!("failed to list media: {error}"))?;
    let mut total_frames = duration_to_frames(minimum_duration, daw_engine::DEFAULT_SAMPLE_RATE)?;
    for track in &project.tracks {
        for clip in &track.clips {
            let clip_end = clip.start_sample.saturating_add(clip.duration_samples);
            if clip_end > start_sample {
                let relative_end = usize::try_from(clip_end - start_sample)
                    .map_err(|_| "clip timeline position is too large".to_owned())?;
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
            let media = project
                .media
                .iter()
                .find(|media| media.id == clip.media_id)
                .ok_or_else(|| {
                    format!(
                        "clip {} references unknown media {}",
                        clip.id, clip.media_id
                    )
                })?;
            let hash = media
                .content_hash
                .as_deref()
                .ok_or_else(|| format!("media {} has no content hash", media.id))?;
            let object = media_objects
                .iter()
                .find(|object| object.hash == hash)
                .ok_or_else(|| format!("media hash {hash} is not imported"))?;
            let path = daw_media::media_object_path(
                project_path.as_ref(),
                &object.hash,
                object.extension.as_deref(),
            );
            let decoded = daw_engine::read_wav(&path)
                .map_err(|error| format!("failed to read media {hash}: {error}"))?;
            if decoded.sample_rate != output.sample_rate {
                return Err(format!(
                    "media {hash} is {} Hz; expected {} Hz",
                    decoded.sample_rate, output.sample_rate
                ));
            }
            let decoded = daw_engine::convert_channels(&decoded, output.channels);
            let source_start = if start_sample > clip.start_sample {
                clip.source_start_sample
                    .saturating_add(start_sample - clip.start_sample)
            } else {
                clip.source_start_sample
            };
            let source_start = usize::try_from(source_start)
                .map_err(|_| "clip source start is too large".to_owned())?;
            let destination_start = if clip.start_sample >= start_sample {
                usize::try_from(clip.start_sample - start_sample)
                    .map_err(|_| "clip start is too large".to_owned())?
            } else {
                0
            };
            let remaining_clip_frames =
                usize::try_from(clip_end - start_sample.max(clip.start_sample))
                    .map_err(|_| "clip duration is too large".to_owned())?;
            let mut limited = slice_buffer_frames(&decoded, source_start, remaining_clip_frames);
            let clip_offset = start_sample
                .max(clip.start_sample)
                .saturating_sub(clip.start_sample);
            daw_engine::apply_clip_fades(
                &mut limited,
                clip_offset,
                clip.duration_samples,
                clip.fade_in_samples,
                clip.fade_out_samples,
            );
            daw_engine::mix_clip(
                &mut output,
                &limited,
                destination_start,
                track.volume_percent,
                track.muted,
            );
        }
    }

    Ok(output)
}

fn slice_buffer_frames(
    buffer: &daw_engine::AudioBuffer,
    start_frame: usize,
    frames: usize,
) -> daw_engine::AudioBuffer {
    daw_engine::slice_frames(buffer, start_frame, frames)
}

fn print_help() {
    println!("daw {VERSION}");
    println!();
    println!("Usage:");
    println!("  daw --version");
    println!("  daw --help");
    println!("  daw init <path> [name]");
    println!("  daw validate <path>");
    println!("  daw inspect <path>");
    println!("  daw project tempo <path> <tempo-bpm>");
    println!("  daw track add <path> <name>");
    println!("  daw track controls <path> <track-id> <volume-percent> <muted> <solo>");
    println!("  daw clip add <path> <track-id> <media-id> <start-sample> <duration-samples>");
    println!("  daw clip move <path> <clip-id> <start-sample> <duration-samples>");
    println!("  daw clip split <path> <clip-id> <split-sample>");
    println!("  daw clip duplicate <path> <clip-id> <start-sample> [track-id]");
    println!("  daw clip fade <path> <clip-id> <fade-in-samples> <fade-out-samples>");
    println!("  daw clip remove <path> <clip-id>");
    println!("  daw snapshot create <path> [message]");
    println!("  daw branch create <path> <name>");
    println!("  daw branch list <path>");
    println!("  daw branch switch <path> <name>");
    println!("  daw vcs init-git <path>");
    println!("  daw vcs remote add <path> <name> <url>");
    println!("  daw vcs status <path>");
    println!("  daw vcs commit <path> <message>");
    println!("  daw vcs push <path> [remote] [branch]");
    println!("  daw vcs pull <path> [remote] [branch]");
    println!("  daw vcs lfs-status <path>");
    println!("  daw media import <path> <source>");
    println!("  daw media list <path>");
    println!("  daw media verify <path>");
    println!("  daw media relink <path> <hash> <replacement>");
    println!("  daw media waveform <path> <hash> [points]");
    println!("  daw media waveforms <path> [points]");
    println!("  daw render-test-tone <output> [duration-seconds]");
    println!("  daw render-metronome <output> <tempo-bpm> [bars] [beats-per-bar]");
    println!("  daw render-project <path> <output> [duration-seconds] [start-sample]");
    println!("  daw play-test-tone [duration-seconds]");
    println!("  daw play-metronome <tempo-bpm> [bars] [beats-per-bar]");
    println!("  daw play-project <path> [minimum-duration-seconds] [start-sample]");
    println!("  daw record-snippet <path> <track-id> [duration-seconds] [start-sample]");
    println!("  daw diff <path> <left-ref> <right-ref>");
    println!("  daw merge <path> <source-branch>");
    println!("  daw history <path>");
    println!("  daw undo <path>");
    println!("  daw redo <path>");
    println!("  daw checkout-snapshot <path> <snapshot-id>");
    println!("  daw replay <path>");
}

fn required_arg(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing required argument: {name}"))
}

fn optional_f32(value: Option<String>, default: f32, name: &str) -> Result<f32, String> {
    value.map_or(Ok(default), |value| {
        value
            .parse::<f32>()
            .map_err(|error| format!("invalid {name}: {error}"))
    })
}

fn optional_u64(value: Option<String>, default: u64, name: &str) -> Result<u64, String> {
    value.map_or(Ok(default), |value| {
        value
            .parse::<u64>()
            .map_err(|error| format!("invalid {name}: {error}"))
    })
}

fn optional_u32(value: Option<String>, default: u32, name: &str) -> Result<u32, String> {
    value.map_or(Ok(default), |value| {
        value
            .parse::<u32>()
            .map_err(|error| format!("invalid {name}: {error}"))
    })
}

fn optional_u16(value: Option<String>, default: u16, name: &str) -> Result<u16, String> {
    value.map_or(Ok(default), |value| {
        value
            .parse::<u16>()
            .map_err(|error| format!("invalid {name}: {error}"))
    })
}

fn optional_usize(value: Option<String>, default: usize, name: &str) -> Result<usize, String> {
    value.map_or(Ok(default), |value| {
        value
            .parse::<usize>()
            .map_err(|error| format!("invalid {name}: {error}"))
    })
}

fn required_u64(args: &mut impl Iterator<Item = String>, name: &str) -> Result<u64, String> {
    required_arg(args, name)?
        .parse::<u64>()
        .map_err(|error| format!("invalid {name}: {error}"))
}

fn required_u16(args: &mut impl Iterator<Item = String>, name: &str) -> Result<u16, String> {
    required_arg(args, name)?
        .parse::<u16>()
        .map_err(|error| format!("invalid {name}: {error}"))
}

fn required_bool(args: &mut impl Iterator<Item = String>, name: &str) -> Result<bool, String> {
    let value = required_arg(args, name)?;
    match value.as_str() {
        "true" | "yes" | "1" | "on" => Ok(true),
        "false" | "no" | "0" | "off" => Ok(false),
        _ => Err(format!("invalid {name}: expected true or false")),
    }
}

#[allow(clippy::cast_precision_loss)]
fn frames_to_seconds(frames: usize, sample_rate: u32) -> f32 {
    (frames as f32 / sample_rate as f32).max(0.0)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn duration_to_frames(duration: f32, sample_rate: u32) -> Result<usize, String> {
    if duration <= 0.0 {
        return Err("duration must be greater than zero".to_owned());
    }
    Ok((f64::from(duration) * f64::from(sample_rate)).round() as usize)
}

fn no_extra_args(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    if let Some(arg) = args.next() {
        Err(format!("unexpected argument: {arg}"))
    } else {
        Ok(())
    }
}

fn default_project_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Untitled")
        .to_owned()
}

fn recording_output_path(project_path: &str, frames_recorded: usize) -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    std::path::Path::new(project_path)
        .join("recordings")
        .join(format!("recording-{timestamp}-{frames_recorded}.wav"))
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn accepts_version_flag() {
        assert!(run(["--version".to_owned()]).is_ok());
    }

    #[test]
    fn rejects_unknown_command() {
        let error = run(["nope".to_owned()]).expect_err("unknown command should fail");

        assert!(error.contains("unknown command"));
    }
}
