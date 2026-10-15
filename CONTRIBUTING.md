# Contributing to PAM_Edit Video

## Setting up

You will need Rust (see `rust-version` in `src/Cargo.toml`), the FFmpeg
7+ development libraries (`brew install ffmpeg`; see `src/README.md` for
Windows and Linux), cmake and a C++ compiler. Then:

```sh
cd src && cargo run -p pamedit
```

That is the editor window. Everything - the engine, the host layer and the
Slint UI - is one Cargo workspace under `src/`.

## Layout

| Path | What lives there |
|---|---|
| `src/crates/` | The engine (core, media, render, export, project), the host layer (`pamedit-host`, `pamedit-speech`), the CLI, and `pamedit`, the Slint editor window |

[`src/README.md`](src/README.md) explains how the crates fit together and
where the sharp edges are. Read it before touching the engine.

## Checks

Run these before opening a PR:

```sh
cd src && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

If you added or changed text the interface shows, also run
`python3 scripts/locales.py` so the string inventory follows; CI checks it.
Translations live in one JSON file per language — see
[`TRANSLATING.md`](TRANSLATING.md).

If you added or changed a downloadable model, put it in
[`models/manifest.toml`](models/manifest.toml) as well as the engine table it
belongs to, and run `python3 scripts/models.py --check`; CI checks that too.
A maintainer then runs the *Mirror models* workflow, which fetches the model,
records its digest in both places and publishes it to the mirror the app
downloads from.

Write what happens through the `log` facade — `log::info!`, `log::warn!`,
`log::error!` — rather than to standard output or standard error. Every run
writes `<app data>/logs/pamedit-<when>.log`, and a packaged build has no
terminal for anything that goes anywhere else; `PAMEDIT_LOG=debug` turns the
level up. See `src/crates/pamedit-host/src/logs.rs`.

New source files need a licence header — see below. Match the style of the code
around you; the engine avoids cleverness on purpose.

## Licensing

PAM_Edit Video is licensed under AGPL-3.0-or-later; see [`LICENSE`](LICENSE) and [`NOTICE.md`](NOTICE.md).

### File headers

Every source file starts with:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
```

Keep your own copyright line if you want one — add it, don't replace what's
there.

### Third-party code

If your patch brings in code you did not write, say so in the PR: what it is,
where it came from, and its licence. Anything incompatible with
AGPL-3.0-or-later cannot be merged, and licence problems are much cheaper to
catch before a merge than after a release.
