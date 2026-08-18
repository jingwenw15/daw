# Implementation Stages

This file records the bottom-up implementation history. The README stays focused
on the general project description and current usage.

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

## Stage 14

The engine can record a fixed-duration snippet from the default input device.
The CLI and UI write the recording as a WAV file, import it into the
content-addressed media store, register it in the project, and add it as a
timeline clip.

```sh
cargo run --bin daw -- record-snippet /tmp/my-session <track-id> 2.0 0
cargo run --bin daw-ui
```

## Stage 15

Recording has a non-blocking transport. The UI can start recording, keep the app
responsive while the input stream runs, and stop recording to finalize/import the
captured WAV as a timeline clip. The fixed-duration CLI command remains
available for smoke tests and scripting.

```sh
cargo run --bin daw-ui
```

## Stage 16

Project rendering and playback can start from a timeline sample. Clips that
begin before the start point are trimmed, clips after the start point are shifted
earlier in the rendered buffer, and the UI exposes a playhead sample field that
can also be copied into the recording insert position.

```sh
cargo run --bin daw -- render-project /tmp/my-session /tmp/from-playhead.wav 1.0 24000
cargo run --bin daw -- play-project /tmp/my-session 1.0 24000
cargo run --bin daw-ui
```

## Stage 17

Imported 16-bit PCM WAV media can generate deterministic waveform preview
caches under `cache/waveforms`. The CLI can build one cache or all supported
media caches, and the UI can generate caches and show cached peak counts next to
media objects. These caches are derived data and can be regenerated from the
content-addressed media store.

```sh
cargo run --bin daw -- media waveform /tmp/my-session <hash> 512
cargo run --bin daw -- media waveforms /tmp/my-session
cargo run --bin daw-ui
```

## Stage 18

The native UI now opens on an arrangement-style workflow instead of a debug
form. The top transport exposes project open/create, play/stop, a prominent
record button, and quick track creation. Recording targets the first track by
default, inserts the captured audio as a clip, generates its waveform cache, and
the main timeline draws waveform clips in track lanes.

```sh
cargo run --bin daw-ui
```

## Stage 19

Recording draws a live waveform preview in the target track lane while the input
stream is running. Existing clips can be dragged horizontally in the arrangement;
the move is committed through the command log when the drag ends, preserving
project history and version-control-friendly state.

```sh
cargo run --bin daw-ui
```

## Stage 20

The arrangement now owns the main editing workflow. Clicking the ruler or a
track lane moves the playhead cursor, and pressing Enter starts playback from
that cursor. Track headers expose volume sliders plus mute/solo buttons, so
common audio controls are next to the track instead of buried in the utility
area. The persistent sidebar has been replaced with a compact bottom utility
strip for status and advanced project tools.

```sh
cargo run --bin daw-ui
```

## Stage 21

Timeline navigation and clip editing are more usable. The transport exposes a
horizontal zoom slider, the arrangement scrolls when zoomed in, clips can be
selected with a highlighted state, and the bottom utility strip shows selected
clip timing. Pressing Space toggles play/stop, and pressing Delete or Backspace
removes the selected clip through the command log.

```sh
cargo run --bin daw-ui
```

## Stage 22

Playback now advances the visible playhead cursor. The UI remembers the sample
where playback started, polls the playback transport, and updates the playhead
while audio is running. Recording also advances the visible playhead from the
live capture buffer. Clip dragging uses clip-specific hit testing and total drag
distance, while empty lane/ruler dragging scrubs the playhead cursor.

```sh
cargo run --bin daw-ui
```

## Stage 23

Track and media cleanup are command-log-backed. Removing a clip prunes media
references that no remaining clip uses, so project media counts stay aligned
with the visible arrangement. Tracks can be removed from the UI track header,
which also removes their clips and prunes now-unused media. Track headers also
include a `Rec` arm button so recordings can target a specific track.

```sh
cargo run --bin daw-ui
```

## Stage 24

Tracks can be renamed inline from the arrangement track header. Renames are
stored as command-log-backed edits and replay correctly, keeping track names
version-control-friendly instead of UI-only state.

```sh
cargo run --bin daw-ui
```

## Stage 25

Clip dragging is now stateful across UI frames. Press and hold a waveform clip,
drag it horizontally to a new timeline position, and release to commit one
command-log-backed move instead of relying on the original hit rectangle.

```sh
cargo run --bin daw-ui
```

## Stage 26

Clip movement can now cross tracks. Dragging an audio clip vertically into
another track lane updates the live preview, and releasing commits a
command-log-backed placement change with both the target track and timeline
start. Recorded snippets use the same clip model after capture, so they can be
moved between tracks too.

```sh
cargo run --bin daw-ui
```

## Stage 27

Project edits now refresh through a verification path in the UI. After track,
media, clip, recording, mixer, rename, and deletion edits, the app reloads the
saved project, replays the command log, and preserves a status message that says
whether the edit verified after reload. The model tests also cover a realistic
record-like workflow: add clips, move one between tracks, delete media-bearing
clips, remove a track, reload, and confirm replay matches each saved state.

```sh
cargo test
cargo run --bin daw-ui
```

## Stage 28

The arrangement has a first-pass timeline snapping mode. The transport exposes a
`Snap` toggle and millisecond grid size, the ruler and track lanes draw subtle
snap grid lines, and playhead clicks plus clip dragging snap to the nearest grid
sample. Project data still stores exact sample positions, so scripts and future
tempo-aware editing can remain sample-accurate.

```sh
cargo test
cargo run --bin daw-ui
```

## Stage 29

Projects now have command-log-backed tempo metadata. New projects default to
120 BPM, validation rejects unusable tempo values, `daw project tempo` can edit
the tempo from scripts, and the native UI exposes a transport BPM control that
commits through the same verified reload path as other project edits. The
timeline remains sample-based for now; tempo is the persistent foundation for
bars/beats, metronome, MIDI, and drum sequencing.

```sh
cargo run --bin daw -- project tempo /tmp/my-session 96
cargo test
cargo run --bin daw-ui
```

## Stage 30

Timeline snapping can now use project tempo. The transport keeps the existing
millisecond snap mode and adds a Beat mode with configurable beat subdivision.
Beat mode derives samples-per-beat from project BPM, draws beat and stronger bar
grid lines in the ruler and track lanes, and snaps playhead/clip edits to the
nearest beat subdivision while keeping project storage sample-accurate.

```sh
cargo test
cargo run --bin daw-ui
```

## Stage 31

The engine can synthesize a metronome click track without bundled samples. Bar
starts use a higher accented decaying sine click, other beats use a lower click,
and the CLI can render or play the generated metronome for a given BPM, bar
count, and beats-per-bar setting.

```sh
cargo run --bin daw -- render-metronome /tmp/metronome.wav 120 4 4
cargo run --bin daw -- play-metronome 120 4 4
cargo test
```

## Stage 32

The native UI can use the generated metronome in transport workflows. A
`Metronome` toggle mixes synthesized clicks into project playback renders, and
recording starts a separate looping click playback transport so timing guidance
is heard without writing the click into the recorded clip file.

```sh
cargo test
cargo run --bin daw-ui
```

## Stage 33

Recording has a non-blocking count-in. The transport exposes a `Count-In` toggle
and bar count; pressing Record plays a generated metronome count-in first, keeps
the UI responsive, and starts input recording automatically when the count-in
playback finishes. Cancelling the count-in stops the pending recording before
any input stream is opened.

```sh
cargo test
cargo run --bin daw-ui
```

## Stage 34

Timeline clips can be edited with trim handles and split at the playhead. The
project model stores a clip source offset so left-edge trims and split clips
play the correct region of the original media, and these edits are replayable
through the command log. The CLI exposes the same split operation.

```sh
cargo run --bin daw -- clip split /tmp/my-session <clip-id> <split-sample>
cargo test
cargo run --bin daw-ui
```

## Stage 35

Selected clips can be duplicated at the playhead. Duplicates preserve the source
media offset and duration, so copied clips keep the audible region from prior
trim or split edits. The CLI exposes the same operation and can optionally place
the duplicate on a target track.

```sh
cargo run --bin daw -- clip duplicate /tmp/my-session <clip-id> <start-sample> [track-id]
cargo test
cargo run --bin daw-ui
```

## Stage 36

Project edits can be undone and redone through deterministic restore commands
in the command log. The CLI exposes top-level `undo` and `redo` commands, and
the native UI exposes transport buttons plus Cmd+Z and Cmd+Shift+Z shortcuts
when text fields are not focused. This first pass stores project states for
restore operations rather than hand-writing inverse logic for every edit type.

```sh
cargo run --bin daw -- undo /tmp/my-session
cargo run --bin daw -- redo /tmp/my-session
cargo test
cargo run --bin daw-ui
```

## Stage 37

Core clip editing actions are visible in a bottom edit toolbar instead of being
buried in advanced project tools. The toolbar shows selected clip timing,
disables unavailable actions, and exposes Undo, Redo, Split, Duplicate, and
Delete controls. Timeline shortcuts now include `S` to split the selected clip
at the playhead and `D` to duplicate it at the playhead when text fields are not
focused.

```sh
cargo test
cargo run --bin daw-ui
```
