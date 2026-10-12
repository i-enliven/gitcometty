## <img alt="GitComet logo" src="assets/gitcomet_logo.svg" width="26" /> GitCometty

A local fork of [GitComet](https://github.com/Auto-Explore/GitComet), a fast
open-source Git GUI built on Zed's [GPUI](https://github.com/zed-industries/zed).
This README covers **building and running it from source**. For upstream
distribution channels, packaging and product pages, go to the upstream repo.

Everything below is verified against this checkout. `CONTRIBUTING.md` has the
full CI and workspace layout; `.github/workflows/` is the ground truth for what
CI actually runs.

### Requirements

- **Git 2.53 or newer** on `PATH`. With an older Git the app shows a notice and
  turns off features that need newer Git (repository maintenance, and so on).
- **Rust 1.99**, pinned by `rust-toolchain.toml` (`channel = "1.99"`, with
  `rustfmt` and `clippy` components). A plain `rustup` install picks it up
  automatically — check with `rustup show active-toolchain`.
- **Python 3** — used by the CI runner and by `scripts/install-mold.py`.
- Network access on a clean checkout: `vendor/` holds only `path =` dependencies
  (the mermaid renderer and the tree-sitter grammars). `gpui-ce` and everything
  else still comes from crates.io and git.

### Prerequisites

**Linux.** The build is *linker-bound*: `.cargo/config.toml` pins the Linux
linker to `scripts/linux/mold-linker.sh`, which runs
`exec clang "$@" -fuse-ld=mold`. Without **both** clang and mold on `PATH` the
build fails at link time.

```bash
# Debian/Ubuntu
sudo apt-get install -y clang pkg-config libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev
# Fedora
sudo dnf install -y clang pkg-config libxcb-devel libxkbcommon-devel libxkbcommon-x11-devel
# Arch
sudo pacman -S clang pkgconf libxcb libxkbcommon libxkbcommon-x11

python3 scripts/install-mold.py --bin-dir "$HOME/.local/bin"
export PATH="$HOME/.local/bin:$PATH"
mold --version    # must report mold 3.0.0
```

To build without mold (slower links, no code change needed):

```bash
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc cargo build -p gitcomet --features ui-gpui,gix
```

Verify the mold linker really was used:
`readelf -p .comment target/debug/gitcomet` should show `mold 3.0.0`.

**macOS.** The system toolchain is enough. Install `git-lfs` if you want the
fixture-backed tests to run rather than skip.

**Windows.** Visual Studio 2022 (Community or Build Tools) with the
**Desktop development with C++** workload — MSVC tools plus a Windows 10/11 SDK.
No separate LLVM: `.cargo/config.toml` routes linking through
`scripts/windows/windows-lld-linker.cmd`, which uses the bundled `rust-lld`.

### Get the checkout complete

Two things are missing from a plain `git clone` and both silently downgrade the
test suite instead of failing:

```bash
git submodule update --init fixtures/syntax_test   # syntax-highlighting corpus
git lfs install                                    # binary test fixtures
```

### Build

```bash
cargo build -p gitcomet --features ui-gpui,gix
```

Produces `target/debug/gitcomet`. `ui-gpui` and `gix` are already the package
defaults (`crates/gitcomet/Cargo.toml`), so the flags are explicit rather than
necessary — drop `ui-gpui` for a headless build with no GUI, which is what CI's
headless lane does:

```bash
cargo build --locked -p gitcomet --no-default-features --features gix
```

Release build, matching the release workflow:

```bash
cargo build -p gitcomet --release --locked --features ui-gpui,gix --bin gitcomet
```

No pre-build codegen step is needed. Build scripts generate the theme registry,
the icon asset table and the splash backdrop PNGs automatically, and the bundled
fonts are committed.

### Run

```bash
cargo run -p gitcomet --bin gitcomet --features ui-gpui,gix -- /path/to/repo
```

or `./target/debug/gitcomet /path/to/repo`. The first argument is the repository
to open; with no argument it falls back to the current directory.

On Linux you need a Wayland or X11 session. Wayland is preferred and the app
falls back to X11; to force X11, unset `WAYLAND_DISPLAY`/`WAYLAND_SOCKET` and
`export XDG_SESSION_TYPE=x11`.

Two runtime notes:

- x86-64 builds pass `-Ctarget-cpu=x86-64-v3` (`.cargo/config.toml`), so the
  binary will not run on older x86-64 CPUs.
- On Windows the renderer defaults to *Automatic*: it probes DX12 and falls back
  to DirectX 11 compatibility, persisting the result. Override with
  `GPUI_WINDOWS_RENDERER=dx11` or `wgpu-dx12` — see `docs/windows-renderer.md`.

### Test

```bash
# Full suite, headless CI mode
cargo test --workspace --no-default-features --features gix

# One crate
cargo test -p gitcomet-ui-gpui --lib

# One test
cargo test -p gitcomet-ui-gpui --lib terminal
```

The GPUI harnesses (`gitcomet-ui-gpui`, `gitcomet-ui-kit`,
`gitcomet-extension-example`) must run under **libtest, one process per crate** —
their test contexts share process-global platform state, so they do not work
under nextest's process-per-test model. Everything else runs under nextest.

The canonical wrapper CI uses handles that split for you, but needs
`cargo-nextest` 0.9.145 (`python3 scripts/ci/install-nextest.py`):

```bash
python3 scripts/ci/run.py compile --context workspace
python3 scripts/ci/run.py test --context workspace
```

### Lint and format

```bash
cargo fmt --all --check
cargo clippy --workspace --no-default-features --features gix -- -D warnings
```

Both are enforced in CI. `unsafe_code = "forbid"` is a workspace lint.

### Using it as a difftool / mergetool

The built binary can be registered with Git for `git difftool` and
`git mergetool`, in both headless (algorithm-only) and GUI modes:

```bash
./target/debug/gitcomet setup      # add --local for this repo only, --dry-run to preview
./target/debug/gitcomet uninstall  # idempotent; restores values it backed up
```

This registers headless and GUI variants with `guiDefault=auto`, so Git opens a
GUI window when a display is available and falls back to headless otherwise.

Manual invocation:

```bash
gitcomet difftool  --local <path> --remote <path> [--path <display_name>]
gitcomet mergetool --local <path> --remote <path> --merged <path> [--base <path>]
```

KDiff3 and Meld invocation forms (`--L1/--L2/--L3`, `-o/--output`, `--base`,
positional arguments) are also accepted, so it can replace either as a
drop-in.

### Themes

Built-in themes are embedded in the binary.

Custom themes are loaded from JSON bundle files in your per-user themes directory,
which the app creates on startup. Schema, file locations and override behaviour:
[THEMES.md](docs/themes.md).

### Crash logs

Panic logs and abnormal-exit recovery state are written to:

- Linux: `~/.local/state/gitcomet/crashes/` (honouring `$XDG_STATE_HOME`)
- macOS: `~/Library/Logs/gitcomet/crashes/`
- Windows: `%LOCALAPPDATA%\gitcomet\crashes\`

See [`docs/crash-symbolication.md`](docs/crash-symbolication.md) for turning a
captured backtrace into something readable.

### Prior work and ideas inspired by

SourceTree, GitKraken, Zed, GPUI, KDiff3, Meld, GitHub Desktop, Git, Gix, Rust,
Smol, and many more.

### License

GitComet is licensed under the GNU Affero General Public License Version 3
(AGPL-3.0-only). See `LICENSE-AGPL-3.0`.

Copyright (C) 2026 AutoExplore Oy — upstream:
[Auto-Explore/GitComet](https://github.com/Auto-Explore/GitComet).