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
