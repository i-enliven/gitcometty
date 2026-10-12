# Terminal panel: hide-on-close and in-app maximize — design

Date: 2026-10-11
Status: approved in conversation by the requester

## Intent and why

The embedded terminal is where you run long-lived things — a dev server, a build, a
`git rebase -i`. Two things about the current panel make it awkward:

1. Closing the panel **kills the shell**. `close_terminal_for_repo`
   (`crates/gitcomet-ui-gpui/src/view/terminal_panel.rs:580`) removes the repo's
   `RepoTerminalSession` and calls `pty.shutdown()`, which makes alacritty's
   event-loop thread drop the `Pty`. The master fd closes, the shell gets SIGHUP,
   the process dies. So you cannot stow the terminal while something runs, and
   bringing it back gives you a fresh shell with no scrollback.
2. The panel is a fixed-height strip at the bottom. Reading a long `git log`, a
   test-run backtrace or a TUI means dragging it taller a little at a time, and
   the commit list and diff above it never get out of the way.

## What the requester decided

- **Closing the panel hides it; the session keeps running.** Reopening shows the
  *same* live session — same PTY, same shell, same scrollback, including whatever
  the program printed while the panel was away.
- **The per-tab `×` still kills that one tab.** The whole-panel `×` hides.
- **A tab closes by itself when its shell exits.** (Already implemented:
  `TerminalBackendEvent::Exit` → `close_terminal_tab_by_session_seq`,
  `terminal_panel.rs:470-486`.)
- **Closing the repo kills its sessions. Closing the window kills them. Quitting
  the app kills them.** Hidden sessions are not immortal.
- **Sessions stay keyed by `RepoId`** on `GitCometView`, exactly as today. No
  workspace-level session store, no per-window reparenting.
- **Maximize is in-app.** It grows the bottom panel **upward**, over the commit
  list and diff area. **Not to the sides** — the sidebar and details pane keep
  their widths and stay put.
- **A control returns from maximize to the panel height it had before.**
- **Scrollback is halved: 5,000 rows**, down from 10,000.

## Goals

- Hiding and reopening a terminal panel is free: no respawn, no lost output, no
  lost scroll position.
- A hidden session costs the application nothing per frame.
- Maximize is one click in, one click out, and the previous height comes back
  exactly.

## Non-goals

- No detached/floating terminal window in an OS window of its own.
- No persistence of panel height or maximize state across launches. Panel height
  is not persisted today either (`terminal_panel_height` appears in no
  `session::UiSettings` / `WorkspaceLayout` field).
- No shell integration (OSC 133 / foreground-job tracking) to improve
  "is a command running" detection.
- No cap on how many sessions may be parked hidden.

## Anti-goals

- A hidden session that keeps printing must not repaint the window. Today a
  `Wakeup` event bumps `content_epoch` and arms a coalesced `cx.notify()` on the
  whole `GitCometView` at up to 60 Hz (`terminal_panel.rs:518-527`). That is
  merely wasteful when the panel is hidden behind the Reflog tab; it becomes a
  real tax once closing the panel stops stopping the session.
- Hiding must not become a half-kill. Either the session is alive and complete,
  or it is gone.

## Constraints

- Rust, GPUI (`zed` GPUI via `vendor/`). No new dependencies.
- Crate under change: `gitcomet-ui-gpui`.
- `cargo clippy --workspace --no-default-features --features gix -- -D warnings`
  stays clean; `cargo fmt` per `.rustfmt.toml`.
- Tests: `cargo test -p gitcomet-ui-gpui --lib [filter]`.
- Icons come from existing assets. `generic_maximize.svg` and
  `generic_restore.svg` already ship (`gitcomet-ui-kit/assets/icons/`) and are
  used by window chrome.

## How it works — the parts the requester decided

**Hide is a flag on the session, not a new container.** `RepoTerminalSession`
(`mod_helpers/mode_impls.rs:1063`) gains `panel_hidden: bool`. Because the flag
lives inside the session, it cannot outlive it: every existing kill path drops
the whole `RepoTerminalSession` and takes the flag with it, so there is no stale
"hidden" entry to reconcile when a repo closes or a window is destroyed.

`bottom_panel_providers::Terminal::is_open` becomes "has an instance **and** is
not hidden", so the panel renders nothing while the session keeps running.
`Terminal::close` sets the flag instead of calling `close_terminal_for_repo`.
`toggle_terminal_for_active_repo` becomes three-way: no session → spawn; session
hidden → unhide and focus; session visible → hide.

**Consequence the requester accepted:** the "N terminals, M running commands —
terminate?" confirmation stops appearing when the panel is closed, because
nothing is being terminated. It still appears for repo close, window close and
quit.

**Maximize is per-window, like the height it replaces.** `terminal_panel_height`
is a single `Pixels` field on `GitCometView`, so maximize state is one `bool`
plus one `Option<Pixels>` beside it. When maximized the panel takes `flex_1`
instead of a fixed height, the resize handle is not rendered, and the main
content above it is not rendered at all. Skipping that render is safe: the commit
list and diff are `Entity<MainPaneView>` and hosted repository views held on the
view (`mod_helpers/mod.rs:1060`, `repository_views.rs:825`), so scroll and
selection survive the unmount.

## Calls the builder made, not the requester

- **Maximize affects the whole bottom panel's height.** Terminal and Reflog share
  one height field, so if Reflog is open it maximizes alongside.
- **One toggle button whose icon swaps** `generic_maximize` ↔ `generic_restore`,
  rather than two separate buttons. Same pattern as the window chrome zoom
  control (`app/chrome.rs:11-31`).
- **No keybinding for maximize.** The requester asked for a button; the existing
  keybinding table in `app/tests.rs:2483` is a shared surface and ctrl-shift-M is
  not obviously free.
- **A hidden terminal keeps its last painted grid size** rather than being
  resized to zero. Grid size only resyncs while painting
  (`sync_terminal_grid_size`, `terminal_panel/viewport.rs:1316`), so a full-screen
  TUI that ran while hidden comes back at the size it last knew, and redraws
  properly on the next real resize.
- **The action-bar Terminal button follows visibility; the repo-tab terminal
  marker follows liveness.** Hidden-but-live is a state that did not exist before.
  The button must read "Show terminal" and read unselected, because it controls
  visibility. The repo-tab marker means "this repository has a terminal", and a
  parked live session genuinely has one — so it stays lit.
- **Repaint suppression for hidden sessions** is extracted into a named predicate
  on `GitCometView` so the rule can be tested directly.
- **Scrollback is pinned by a value test.** `TERMINAL_SCROLLBACK_ROWS` is a
  constant with no behavioural test surface; the test asserts the requested
  number so a future edit cannot drift it silently.