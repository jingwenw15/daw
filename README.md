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
