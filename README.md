# DAW

An open-source digital audio workstation built bottom-up with a Rust core,
project-native version control, and a real-time-aware audio architecture.

The project is intentionally incremental: each capability should leave the DAW
in a working state, even when the feature is still minimal. The primary testing
platform is macOS, with the architecture kept portable where practical.

## Goals

- Build a stable DAW foundation without copying proprietary DAW code.
- Keep project data transparent, replayable, and friendly to Git/private repos.
- Separate real-time audio work from UI and project-management work.
- Make common recording and editing workflows intuitive before layering on
  advanced tools.
- Treat imported media as content-addressed assets and generated waveform data
  as disposable cache.

## Current Capabilities

- Native desktop UI for creating/opening projects.
- Track creation, deletion, renaming, arming, volume, mute, and solo controls.
- Audio recording from the default input device.
- Timeline playback from a movable playhead.
- Command-log-backed project tempo.
- Generated metronome click rendering and playback.
- Native UI metronome during playback and recording.
- Non-blocking recording count-in.
- Imported WAV media, waveform cache generation, and waveform drawing.
- Clip selection, deletion, command-log-backed placement edits, horizontal
  timeline dragging, trim handles, split-at-playhead editing, and clip
  duplication.
- Visible edit toolbar with shortcuts for common timeline clip actions.
- Command-log-backed undo and redo for project edits.
- Time-based and tempo-derived beat-grid snapping in the arrangement.
- Optional Git integration through system Git, including private remote support
  via the user's existing credentials.

## Usage

Run the CLI tests:

```sh
cargo test
```

Launch the native UI:

```sh
cargo run --bin daw-ui
```

Create and inspect a project from the CLI:

```sh
cargo run --bin daw -- init /tmp/my-session "My Session"
cargo run --bin daw -- validate /tmp/my-session
cargo run --bin daw -- inspect /tmp/my-session
```

Generate a test tone and use it as timeline media:

```sh
cargo run --bin daw -- render-test-tone /tmp/test-tone.wav 1.0
cargo run --bin daw -- media import /tmp/my-session /tmp/test-tone.wav
```

## Documentation

The implementation history is tracked separately in
[docs/STAGES.md](docs/STAGES.md).
