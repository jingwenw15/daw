# DAW

An open-source DAW built bottom-up with a Rust core, project-native version control, and real-time-safe audio architecture.

## Stage 0

This repository currently contains the initial Rust workspace and a minimal CLI.

```sh
cargo test
cargo run --bin daw -- --version
```

## Stage 1

The CLI can now create and inspect a minimal project directory.

```sh
cargo run --bin daw -- init /tmp/my-session "My Session"
cargo run --bin daw -- validate /tmp/my-session
cargo run --bin daw -- inspect /tmp/my-session
```

## Stage 2

Project edits now flow through an append-only command log, and full project
snapshots can be created and restored.

```sh
cargo run --bin daw -- track add /tmp/my-session "Drums"
cargo run --bin daw -- snapshot create /tmp/my-session "before chorus edit"
cargo run --bin daw -- history /tmp/my-session
cargo run --bin daw -- replay /tmp/my-session
cargo run --bin daw -- checkout-snapshot /tmp/my-session <snapshot-id>
```

## Stage 3

Local branches/takes can now be created, switched, diffed, and merged for
non-conflicting track additions.

```sh
cargo run --bin daw -- branch create /tmp/my-session chorus
cargo run --bin daw -- branch switch /tmp/my-session chorus
cargo run --bin daw -- diff /tmp/my-session current branch:main
cargo run --bin daw -- merge /tmp/my-session chorus
```

## Stage 4

Git integration is optional and uses system Git, so private SSH and HTTPS
remotes rely on the user's existing Git credentials and macOS credential setup.

```sh
cargo run --bin daw -- vcs init-git /tmp/my-session
cargo run --bin daw -- vcs remote add /tmp/my-session origin git@github.com:you/private-daw-project.git
cargo run --bin daw -- vcs status /tmp/my-session
cargo run --bin daw -- vcs commit /tmp/my-session "initial project"
cargo run --bin daw -- vcs push /tmp/my-session origin main
cargo run --bin daw -- vcs pull /tmp/my-session origin main
cargo run --bin daw -- vcs lfs-status /tmp/my-session
```

## Stage 5

Media import uses content-addressed SHA-256 storage plus a JSON media index.
Waveform/cache data is disposable under `cache/`.

```sh
cargo run --bin daw -- media import /tmp/my-session /path/to/kick.wav
cargo run --bin daw -- media list /tmp/my-session
cargo run --bin daw -- media verify /tmp/my-session
cargo run --bin daw -- media relink /tmp/my-session <hash> /path/to/replacement.wav
```

## Stage 6

The offline engine can render deterministic WAV files. `render-test-tone`
generates a 440 Hz sine tone, and `render-project` currently validates the
project and writes a silent placeholder render until timeline clips are added.

```sh
cargo run --bin daw -- render-test-tone /tmp/test-tone.wav 1.0
cargo run --bin daw -- render-project /tmp/my-session /tmp/project-render.wav 1.0
```

## Stage 7

The default output device can play a short real-time test tone. On macOS this
uses CoreAudio through the engine backend.

```sh
cargo run --bin daw -- play-test-tone 1.0
```

## Stage 8

Projects can place imported 16-bit PCM WAV media on tracks and render/play the
timeline. Timeline positions are sample-frame based at the engine render rate.

```sh
cargo run --bin daw -- media import /tmp/my-session /tmp/test-tone.wav
cargo run --bin daw -- clip add /tmp/my-session <track-id> <media-id> 0 48000
cargo run --bin daw -- render-project /tmp/my-session /tmp/timeline.wav 1.0
cargo run --bin daw -- play-project /tmp/my-session 1.0
```

## Stage 9

A minimal native UI shell can create/open projects, add tracks, create
snapshots, inspect tracks/media/history, validate the project, and trigger a
short playback smoke tone.

```sh
cargo run --bin daw-ui
```

## Stage 10

The engine exposes a cancellable playback transport. The UI now renders the
loaded project timeline for playback and its Stop button owns a real transport
handle instead of waiting for a fixed test tone to finish.

```sh
cargo run --bin daw-ui
```

## Stage 11

The native UI can import local 16-bit PCM WAV media into the content-addressed
media store, register it in the command-log-backed project model, add clips to
tracks, and play/stop the resulting timeline.

```sh
cargo run --bin daw -- render-test-tone /private/tmp/test-tone.wav 2.0
cargo run --bin daw-ui
```

## Stage 12

Track mixer controls are command-log-backed. The CLI and UI can set volume,
mute, and solo values, and project render/playback honors those settings.

```sh
cargo run --bin daw -- track controls /tmp/my-session <track-id> 75 false true
cargo run --bin daw-ui
```

## Stage 13

Timeline clip placement edits are command-log-backed. Clips can be moved,
resized, or removed through the CLI and UI, and replay rebuilds those edits.

```sh
cargo run --bin daw -- clip move /tmp/my-session <clip-id> 12000 36000
cargo run --bin daw -- clip remove /tmp/my-session <clip-id>
cargo run --bin daw-ui
```
