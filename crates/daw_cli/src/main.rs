//! Command-line entrypoint for the DAW.

use std::{env, process::ExitCode};

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
        Some("track") => run_track(args),
        Some("snapshot") => run_snapshot(args),
        Some("branch") => run_branch(args),
        Some("vcs") => run_vcs(args),
        Some("media") => run_media(args),
        Some("render-test-tone") => run_render_test_tone(args),
        Some("render-project") => run_render_project(args),
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

fn run_render_project(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let project_path = required_arg(&mut args, "path")?;
    let output = required_arg(&mut args, "output")?;
    let duration = optional_f32(args.next(), 1.0, "duration-seconds")?;
    no_extra_args(args)?;
    daw_model::load_project(project_path.as_ref())
        .map_err(|error| format!("project is invalid: {error}"))?;
    let buffer = daw_engine::render_silence(
        duration,
        daw_engine::DEFAULT_SAMPLE_RATE,
        daw_engine::DEFAULT_CHANNELS,
    )
    .map_err(|error| format!("failed to render project: {error}"))?;
    daw_engine::write_wav(output.as_ref(), &buffer)
        .map_err(|error| format!("failed to write project render: {error}"))?;
    println!(
        "rendered project placeholder: {} frames at {} Hz -> {}",
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
            println!("imported media {}", object.hash);
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
        Some(command) => Err(format!(
            "unknown media command: {command}\nrun `daw --help` for usage"
        )),
        None => Err("missing media command\nrun `daw --help` for usage".to_owned()),
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

fn print_help() {
    println!("daw {VERSION}");
    println!();
    println!("Usage:");
    println!("  daw --version");
    println!("  daw --help");
    println!("  daw init <path> [name]");
    println!("  daw validate <path>");
    println!("  daw inspect <path>");
    println!("  daw track add <path> <name>");
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
    println!("  daw render-test-tone <output> [duration-seconds]");
    println!("  daw render-project <path> <output> [duration-seconds]");
    println!("  daw diff <path> <left-ref> <right-ref>");
    println!("  daw merge <path> <source-branch>");
    println!("  daw history <path>");
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
