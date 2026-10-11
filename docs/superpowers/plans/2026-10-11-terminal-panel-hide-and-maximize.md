# Terminal Panel Hide-on-Close and In-App Maximize Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This repo's requester has additionally specified an **orchestrator** execution model (see `~/.agents/roles/orchestrator.md`): a coordinator dispatches one coder per task, then a reviewer on the diff, then a tester, and runs the final gate itself. Task boundaries below are the dispatch boundaries.

**Goal:** Closing the embedded terminal's bottom panel hides it instead of killing the shell, and a maximize control grows that panel upward over the commit-list/diff area and back.

**Architecture:** All work is inside `GitCometView` (`crates/gitcomet-ui-gpui`). Hide-on-close is one `bool` added to the existing per-repo `RepoTerminalSession`, so the flag dies with the session and every existing kill path (repo close, window close, app quit) keeps working untouched. Maximize is two fields on `GitCometView` plus a render branch that swaps the panel's fixed height for `flex_1` and drops the main content above it. Scrollback is one constant.

**Tech Stack:** Rust, Zed GPUI (`gpui`, vendored), `alacritty_terminal` 0.26, `rustc_hash`, `gpui::test` harness.

**Spec:** `docs/superpowers/specs/2026-10-11-terminal-panel-hide-and-maximize-design.md`

## Global Constraints

- Crate under change: `gitcomet-ui-gpui`. No new dependencies, no `Cargo.toml` edits.
- Nothing is persisted. No field is added to `session::UiSettings`, `session::UiSessionFile`, `session::WorkspaceLayout`, or `view::UiPreferences`.
- `TERMINAL_SCROLLBACK_ROWS = 5_000` exactly.
- Panel minimum height stays `TERMINAL_PANEL_MIN_HEIGHT_PX = 120.0` (`terminal_panel.rs:17`); default stays `TERMINAL_PANEL_DEFAULT_HEIGHT_PX = 220.0` (`view/mod.rs:500`).
- Icons are existing assets only: `icons/generic_maximize.svg`, `icons/generic_restore.svg`. No new SVG, no asset registration code.
- Field visibility in `mod_helpers/mod.rs`'s `GitCometView` cluster is `pub(super)`; methods reached from sibling `view` submodules are `pub(in crate::view)`.
- Test command for this crate: `cargo test -p gitcomet-ui-gpui --lib <filter>`. Full suite: `cargo test --workspace --no-default-features --features gix`.
- Lint gate: `cargo clippy --workspace --no-default-features --features gix -- -D warnings`.
- Every test starts with `let _visual_guard = crate::test_support::lock_visual_test();` when it draws, and uses the helpers in `terminal_panel/tests/support.rs`.
- Commit at the end of each task.

## Review Focus

The spec implies these; no single task's headline test names them. Each line gets its test in the task that owns the code.

1. **A session parked hidden that keeps printing** — expect: its output still lands in scrollback, and the window does not repaint frame after frame.
2. **Reopening a hidden panel** — expect: the *same* shell (same `session_seq`, same viewport entity), not a respawned one with empty scrollback.
3. **A hidden session when the repo tab closes, the window closes, or the app quits** — expect: it dies. Hiding must not make a session immortal.
4. **Maximize, then restore** — expect: the panel's width is byte-identical before and after (it grows up, not sideways), and the commit list/diff reappears where it was.
5. **The action-bar Terminal button and the repo-tab terminal marker while a session is hidden** — expect: the button reads unselected with tooltip "Show terminal"; the repo tab still shows its terminal marker.

---

## Task 1: Halve the terminal scrollback

**Files:**
- Modify: `crates/gitcomet-ui-gpui/src/view/terminal_alacritty.rs:18` (constant), and add a test beside `#[test] fn terminal_config_disables_osc52()` at `terminal_alacritty.rs:1742`
- Test: same file, inline `#[test]`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub(super) const TERMINAL_SCROLLBACK_ROWS: usize = 5_000;` — read unchanged by `terminal_alacritty.rs:273`, `:289`, `terminal_panel/viewport.rs:1338`.

- [ ] **Step 1: Write the failing value test**

Add next to `terminal_config_disables_osc52` (`terminal_alacritty.rs:1742`). This is deliberately a value-pinning test: the constant has no behavioural test surface, and the requester specified the number.

```rust
#[test]
fn terminal_scrollback_keeps_five_thousand_rows() {
    assert_eq!(TERMINAL_SCROLLBACK_ROWS, 5_000);
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p gitcomet-ui-gpui --lib terminal_scrollback_keeps_five_thousand_rows`
Expected: FAIL, left-hand side `10000`, right-hand side `5000`.

- [ ] **Step 3: Change the constant**

`terminal_alacritty.rs:18` → `pub(super) const TERMINAL_SCROLLBACK_ROWS: usize = 5_000;`. Touch nothing else; the three use sites read the constant.

- [ ] **Step 4: Run the terminal tests and confirm they pass**

Run: `cargo test -p gitcomet-ui-gpui --lib terminal`
Expected: PASS, including `terminal_config_disables_osc52`, `selection_grid_point_resolves_scrollback_rows_as_negatives`, `grid_point_ordering_holds_across_scrollback`, and `selecting_after_scrolling_back_copies_the_scrollback_rows`. Tests use their own `TEST_SCROLLBACK = 100` (`terminal_panel/tests/support.rs:10`), so none of them should move.

- [ ] **Step 5: Commit**

```bash
git add crates/gitcomet-ui-gpui/src/view/terminal_alacritty.rs
git commit -m "Reduce embedded terminal scrollback to 5000 rows"
```

---

## Task 2: Closing the panel hides the session instead of killing it

**Files:**
- Modify: `crates/gitcomet-ui-gpui/src/view/mod_helpers/mode_impls.rs:1063-1068` (`RepoTerminalSession`)
- Modify: `crates/gitcomet-ui-gpui/src/view/terminal_panel.rs:219-238` (`toggle_terminal_for_active_repo`), `:347-356` (session construction), and add the hide/show methods near `close_terminal_for_repo` (`:580`)
- Modify: `crates/gitcomet-ui-gpui/src/view/bottom_panel_providers.rs:38-56` (`Terminal::is_open`, `Terminal::close`)
- Modify: `crates/gitcomet-ui-gpui/src/view/terminal_panel/tests/support.rs:76` (`test_terminal_session` construction)
- Test: `crates/gitcomet-ui-gpui/src/view/terminal_panel/tests/lifecycle.rs`

**Interfaces:**
- Consumes: `GitCometView::terminal_sessions: FxHashMap<RepoId, RepoTerminalSession>`, `focus_terminal_view(repo_id, window, cx)`, `reset_terminal_cursor_blink(cx)`, `sync_terminal_indicator_views(cx)`, `active_bottom_panel: FxHashMap<RepoId, BottomPanelTab>`.
- Produces:
  - `RepoTerminalSession::panel_hidden: bool` — read by Tasks 3 and 4.
  - `pub(in crate::view) fn hide_terminal_panel_for_repo(&mut self, repo_id: RepoId, cx: &mut gpui::Context<Self>)`
  - `pub(in crate::view) fn show_terminal_panel_for_repo(&mut self, repo_id: RepoId, window: &mut Window, cx: &mut gpui::Context<Self>)`

- [ ] **Step 1: Write the failing test for hide**

Append to `terminal_panel/tests/lifecycle.rs`. Follow `shell_exit_of_the_last_tab_closes_the_terminal_panel` (`lifecycle.rs:174-204`) for setup shape.

```rust
#[gpui::test]
fn closing_the_terminal_panel_hides_it_and_keeps_the_session_alive(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    cx.update(|window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(50, Some(events_rx))], 0, cx),
            );
            this.hide_terminal_panel_for_repo(repo_id, cx);

            let theme = this.theme;
            assert!(this.render_terminal_panel(theme, window, cx).is_none());
            let session = this
                .terminal_sessions
                .get(&repo_id)
                .expect("hidden session survives panel close");
            assert!(session.panel_hidden);
            assert!(session.instance_by_seq(50).is_some());
            drop(events_tx);
        })
    });
}
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p gitcomet-ui-gpui --lib closing_the_terminal_panel_hides_it_and_keeps_the_session_alive`
Expected: compile failure — `no field panel_hidden on type RepoTerminalSession` and `no method hide_terminal_panel_for_repo`.

- [ ] **Step 3: Add the flag and its two construction sites**

In `mod_helpers/mode_impls.rs`, add to `RepoTerminalSession` (`:1063`):

```rust
    /// True when the bottom panel is closed for this repo while the session
    /// keeps running. Lives on the session so it cannot outlive it.
    pub(crate) panel_hidden: bool,
```

Set `panel_hidden: false` at both construction sites: `terminal_panel.rs:351` (`open_terminal_for_repo`'s `RepoTerminalSession { … }` literal) and `terminal_panel/tests/support.rs:76` (`test_terminal_session`).

- [ ] **Step 4: Add `hide_terminal_panel_for_repo`**

In `terminal_panel.rs`, beside `close_terminal_for_repo` (`:580`). It sets the flag, keeps everything else: no `pty.shutdown()`, no `terminal_sessions.remove`, no cursor-blink teardown.

```rust
    pub(in crate::view) fn hide_terminal_panel_for_repo(
        &mut self,
        repo_id: RepoId,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(session) = self.terminal_sessions.get_mut(&repo_id) {
            session.panel_hidden = true;
        }
        self.sync_terminal_indicator_views(cx);
        cx.notify();
    }
```

- [ ] **Step 5: Add `show_terminal_panel_for_repo`**

```rust
    pub(in crate::view) fn show_terminal_panel_for_repo(
        &mut self,
        repo_id: RepoId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = self.terminal_sessions.get_mut(&repo_id) else {
            return;
        };
        session.panel_hidden = false;
        self.active_bottom_panel
            .insert(repo_id, BottomPanelTab::Terminal);
        self.reset_terminal_cursor_blink(cx);
        self.sync_terminal_indicator_views(cx);
        self.focus_terminal_view(repo_id, window, cx);
        cx.notify();
    }
```

- [ ] **Step 6: Make the provider report and set visibility**

In `bottom_panel_providers.rs`, `Terminal::is_open` (`:38`) becomes "has an instance **and** is not hidden", and `Terminal::close` (`:53`) hides instead of asking to terminate:

```rust
    fn is_open(&self, root: &GitCometView, repo: RepoId, _: &App) -> bool {
        root.terminal_sessions
            .get(&repo)
            .is_some_and(|session| !session.panel_hidden && session.active_instance().is_some())
    }

    fn close(&self, root: &mut GitCometView, repo: RepoId, cx: &mut gpui::Context<GitCometView>) {
        root.hide_terminal_panel_for_repo(repo, cx);
    }
```

The `request_close_terminal_for_repo` / `close_terminal_for_repo` pair disappears from this provider entirely; both remain in use by `perform_close_action`'s confirmed-shutdown route (`terminal_panel.rs:916`).

- [ ] **Step 7: Make the toggle three-way**

Replace the `if self.terminal_sessions.contains_key(&repo_id) { … }` block in `toggle_terminal_for_active_repo` (`terminal_panel.rs:231-236`) with: hidden → show; visible → hide; absent → `open_terminal_for_repo`. Keep the existing early return when there is no active repo, and keep `workdir` / `repo_name` computation for the spawn branch.

- [ ] **Step 8: Run the hide test and confirm it passes**

Run: `cargo test -p gitcomet-ui-gpui --lib closing_the_terminal_panel_hides_it_and_keeps_the_session_alive`
Expected: PASS

- [ ] **Step 9: Write the remaining lifecycle tests**

All in `terminal_panel/tests/lifecycle.rs`, same setup shape. Name them exactly:

```rust
#[gpui::test]
fn showing_a_hidden_terminal_panel_restores_the_same_session(cx: &mut gpui::TestAppContext)
```
Hide a session holding `session_seq` 51, then `show_terminal_panel_for_repo`. Assert `!session.panel_hidden`, `session.instance_by_seq(51).is_some()`, `render_terminal_panel(...)` is `Some`, and `active_bottom_panel.get(&repo_id) == Some(&BottomPanelTab::Terminal)`.

```rust
#[gpui::test]
fn the_terminal_toggle_hides_a_visible_panel_and_reshows_a_hidden_one(cx: &mut gpui::TestAppContext)
```
Insert a visible session, call `toggle_terminal_for_active_repo` twice. Assert `panel_hidden` is `true` after the first call and `false` after the second, and that `terminal_sessions` still contains `repo_id` throughout with the same `session_seq` — i.e. no respawn.

```rust
#[gpui::test]
fn closing_the_repo_still_kills_a_hidden_terminal_session(cx: &mut gpui::TestAppContext)
```
Hide, then `close_terminal_for_repo(repo_id, cx)`. Assert `!this.terminal_sessions.contains_key(&repo_id)`.

```rust
#[gpui::test]
fn hiding_the_terminal_panel_asks_no_terminate_confirmation(cx: &mut gpui::TestAppContext)
```
Hide a session. Assert `this.pending_terminal_shutdown_prompt.is_none()` (`mod_helpers/mod.rs:1216`) and that `render_bottom_panel(theme, window, cx)` is `None`.

```rust
#[gpui::test]
fn closing_a_tab_still_kills_that_tab_and_leaves_the_panel_visible(cx: &mut gpui::TestAppContext)
```
Insert a session holding `session_seq` 52 and 53 with `active_index` 1, then `close_terminal_tab(repo_id, 1, window, cx)` (`terminal_panel.rs:598`). Assert `instance_by_seq(53)` is `None`, `instance_by_seq(52)` is `Some`, `!session.panel_hidden`, and `render_terminal_panel(...)` is `Some`. This pins the requester's split: the tab `×` destroys, the panel `×` hides.

- [ ] **Step 10: Run the whole terminal suite and confirm nothing regressed**

Run: `cargo test -p gitcomet-ui-gpui --lib terminal`
Expected: PASS. Pay attention to `terminal_toolbar_fill_tracks_panel_lifetime_and_target` (`lifecycle.rs:5`), `terminal_shutdown_confirm_click_closes_the_panel` (`shutdown_confirmation.rs:68`) and `terminal_session_disappearance_dismisses_menu_and_rejects_late_actions` (`context_menu.rs:668`) — all three must still pass unchanged. If `terminal_shutdown_confirm_click_closes_the_panel` now fails, it is reaching the panel-close path through the old kill route; fix the test's entry point to the shutdown-prompt route it names, not the production code.

- [ ] **Step 11: Lint, format, commit**

```bash
cargo fmt
cargo clippy -p gitcomet-ui-gpui --no-default-features --features gix -- -D warnings
cargo test -p gitcomet-ui-gpui --lib
git add -A && git commit -m "Hide the terminal panel on close instead of killing the session"
```

---

## Task 3: Make a hidden session cost nothing and read correctly

**Files:**
- Modify: `crates/gitcomet-ui-gpui/src/view/terminal_panel.rs:450-577` (`spawn_terminal_event_task`'s `Wakeup | CursorBlinkingChange` arm and the delayed repaint task), plus the new predicate
- Modify: `crates/gitcomet-ui-gpui/src/view/terminal_panel.rs:275-289` (`sync_terminal_indicator_views`)
- Modify: `crates/gitcomet-ui-gpui/src/view/panels/repo_tabs_bar.rs:16, 435, 484-493, 810, 996`
- Test: `crates/gitcomet-ui-gpui/src/view/terminal_panel/tests/render_coalescing.rs`, `crates/gitcomet-ui-gpui/src/view/terminal_panel/tests/lifecycle.rs`

**Interfaces:**
- Consumes: `RepoTerminalSession::panel_hidden` (Task 2).
- Produces:
  - `pub(in crate::view) fn terminal_repaint_should_schedule(&self, repo_id: RepoId, session_seq: u64) -> bool`
  - `pub(in super::super) fn set_live_terminal_repo_ids(&mut self, next: FxHashSet<RepoId>, cx: &mut gpui::Context<Self>)` on `RepoTabsBarView`

- [ ] **Step 1: Write the failing predicate tests**

In `terminal_panel/tests/render_coalescing.rs`. The rule is extracted into a method precisely so it can be tested without racing a repaint timer.

```rust
#[gpui::test]
fn a_hidden_terminal_session_schedules_no_repaint(cx: &mut gpui::TestAppContext)
```
Insert a visible session with `session_seq` 60. Assert `this.terminal_repaint_should_schedule(repo_id, 60)`. Set `session.panel_hidden = true`. Assert `!this.terminal_repaint_should_schedule(repo_id, 60)`.

```rust
#[gpui::test]
fn a_missing_terminal_session_schedules_no_repaint(cx: &mut gpui::TestAppContext)
```
Assert `!this.terminal_repaint_should_schedule(repo_id, 61)` with no session inserted, and again with a session inserted that holds only `session_seq` 60, queried at 61.

- [ ] **Step 2: Run them and confirm they fail**

Run: `cargo test -p gitcomet-ui-gpui --lib schedules_no_repaint`
Expected: compile failure — `no method terminal_repaint_should_schedule`.

- [ ] **Step 3: Add the predicate and route the event task through it**

```rust
    /// Whether a PTY wakeup for this instance should arm a coalesced repaint.
    /// A hidden panel paints nothing, so a parked session must not wake the
    /// whole window at up to 60 Hz.
    pub(in crate::view) fn terminal_repaint_should_schedule(
        &self,
        repo_id: RepoId,
        session_seq: u64,
    ) -> bool {
        self.terminal_sessions
            .get(&repo_id)
            .filter(|session| !session.panel_hidden)
            .and_then(|session| session.instance_by_seq(session_seq))
            .is_some()
    }
```

In `spawn_terminal_event_task`'s `Wakeup | CursorBlinkingChange` arm (`terminal_panel.rs:518-527`): keep bumping `viewport.content_epoch` unconditionally — a hidden session must still be marked dirty so the first paint after unhiding rebuilds from the real buffer — but return `false` from the closure when `session.panel_hidden`, so `coalescer.mark_dirty()` is never armed. Then make the delayed repaint task's `entity.update` body bail out through the same predicate, so a burst armed before hiding cannot notify after hiding.

- [ ] **Step 4: Run the predicate tests and confirm they pass**

Run: `cargo test -p gitcomet-ui-gpui --lib schedules_no_repaint`
Expected: PASS

- [ ] **Step 5: Write the failing output-still-tracked test**

In `terminal_panel/tests/lifecycle.rs`:

```rust
#[gpui::test]
fn a_hidden_terminal_still_tracks_incoming_output(cx: &mut gpui::TestAppContext)
```
Insert a session with `session_seq` 62 and its `events_rx`, `spawn_terminal_event_task`, hide the panel, `drop` the flag-setting borrow, then `events_tx.try_send(TerminalBackendEvent::Wakeup)` twice and `cx.run_until_parked()`. Assert the viewport's `content_epoch` advanced by 2 (`terminal_viewport_for_session(repo_id, 62)` → `read(cx).content_epoch`). This is Review Focus line 1's other half: suppression must not stop the session from tracking output.

- [ ] **Step 6: Run it and confirm it passes**

Run: `cargo test -p gitcomet-ui-gpui --lib a_hidden_terminal_still_tracks_incoming_output`
Expected: PASS. If it fails, the `content_epoch` bump was moved inside the hidden guard — move it back out; only the repaint arming is suppressed.

- [ ] **Step 7: Split the two indicator sets**

`sync_terminal_indicator_views` (`terminal_panel.rs:275`) currently sends `terminal_sessions.keys()` to both bars. Compute two sets and send each where it belongs — Review Focus line 5:

```rust
        let live: FxHashSet<RepoId> = self.terminal_sessions.keys().copied().collect();
        let visible: FxHashSet<RepoId> = self
            .terminal_sessions
            .iter()
            .filter(|(_, session)| !session.panel_hidden)
            .map(|(repo_id, _)| *repo_id)
            .collect();
```

Send `visible` to `action_bar.set_open_terminal_repo_ids` (unchanged setter, unchanged meaning: the button controls visibility, so hidden must read unselected with tooltip "Show terminal" — `panels/action_bar.rs:1419-1442`). Send `live` to a new `repo_tabs_bar.set_live_terminal_repo_ids`.

- [ ] **Step 8: Add the live set to the repo tabs bar**

Mirror `set_open_terminal_repo_ids` (`repo_tabs_bar.rs:484-493`) exactly: add `live_terminal_repo_ids: FxHashSet<RepoId>` beside the existing field (`:16`), initialise it at `:435`, add the setter with the same early-return-if-unchanged + `cx.notify()` shape, and use `live_terminal_repo_ids` at the two render sites (`:810` marker width, `:996` marker visibility). Leave `open_terminal_repo_ids` on `RepoTabsBarView` in place only if it is still read elsewhere; if `live` fully replaces it there, remove the now-dead field and setter rather than leaving two names for one thing.

- [ ] **Step 9: Write the indicator test**

In `terminal_panel/tests/lifecycle.rs`:

```rust
#[gpui::test]
fn a_hidden_terminal_reads_as_closed_on_the_action_bar_and_live_on_the_repo_tab(cx: &mut gpui::TestAppContext)
```
Insert a visible session, hide it, `cx.run_until_parked()` (the indicator sync is deferred — `terminal_panel.rs:282`). Assert the action bar's `open_terminal_repo_ids` does not contain `repo_id` and the repo tabs bar's `live_terminal_repo_ids` does. Both fields are private; read them through the crate's existing test accessors if present, otherwise make the two fields `pub(in crate::view)` in this task and assert directly.

- [ ] **Step 10: Run the suite and confirm it passes**

Run: `cargo test -p gitcomet-ui-gpui --lib terminal`
Expected: PASS

- [ ] **Step 11: Lint, format, commit**

```bash
cargo fmt
cargo clippy -p gitcomet-ui-gpui --no-default-features --features gix -- -D warnings
cargo test -p gitcomet-ui-gpui --lib
git add -A && git commit -m "Stop hidden terminal sessions from repainting and split terminal indicators"
```

---

## Task 4: Maximize the bottom panel upward, and restore its height

**Files:**
- Modify: `crates/gitcomet-ui-gpui/src/view/mod_helpers/mod.rs:1131` (two fields), `crates/gitcomet-ui-gpui/src/view/gitcomet_view.rs:1959` (initialisers)
- Modify: `crates/gitcomet-ui-gpui/src/view/terminal_panel.rs:1137-1156` (panel height vs `flex_1`), `:1320-1389` (new toggle button), `:1448-1516` (`.debug_selector` on the resize handle)
- Modify: `crates/gitcomet-ui-gpui/src/view/reflog_panel.rs:131-172` (the two `.h(...)` wrappers)
- Modify: `crates/gitcomet-ui-gpui/src/view/splash.rs:747-752, 858-873`
- Create: `crates/gitcomet-ui-gpui/src/view/terminal_panel/tests/maximize.rs`; register `mod maximize;` in `terminal_panel/tests/mod.rs`

**Interfaces:**
- Consumes: `terminal_panel_height: Pixels`, `terminal_sessions`, `RepoTerminalSession::panel_hidden` (Task 2), `TERMINAL_PANEL_DEFAULT_HEIGHT_PX`.
- Produces:
  - `pub(super) terminal_panel_maximized: bool`
  - `pub(super) terminal_panel_height_before_maximize: Option<Pixels>`
  - `pub(in crate::view) fn toggle_terminal_panel_maximized(&mut self, cx: &mut gpui::Context<Self>)`

- [ ] **Step 1: Write the failing state tests**

In the new `terminal_panel/tests/maximize.rs`:

```rust
#[gpui::test]
fn maximizing_the_terminal_panel_and_restoring_returns_the_previous_height(cx: &mut gpui::TestAppContext)
```
Set `terminal_panel_height` to `px(300.0)`, call `toggle_terminal_panel_maximized(cx)`. Assert `terminal_panel_maximized == true` and `terminal_panel_height_before_maximize == Some(px(300.0))`. Call it again. Assert `terminal_panel_maximized == false`, `terminal_panel_height_before_maximize.is_none()`, and `terminal_panel_height == px(300.0)`.

```rust
#[gpui::test]
fn restoring_a_maximized_panel_without_a_saved_height_uses_the_default(cx: &mut gpui::TestAppContext)
```
Set `terminal_panel_maximized = true` and `terminal_panel_height_before_maximize = None` by hand, call the toggle, assert `terminal_panel_height == px(TERMINAL_PANEL_DEFAULT_HEIGHT_PX)`.

- [ ] **Step 2: Run them and confirm they fail**

Run: `cargo test -p gitcomet-ui-gpui --lib maximizing_the_terminal_panel`
Expected: compile failure — `no field terminal_panel_maximized`.

- [ ] **Step 3: Add the fields, initialisers and the toggle**

Fields beside `terminal_panel_height` (`mod_helpers/mod.rs:1131`), `pub(super)`, with a doc-comment naming the per-window scope:

```rust
    pub(super) terminal_panel_maximized: bool,
    pub(super) terminal_panel_height_before_maximize: Option<Pixels>,
```

Initialise `false` / `None` in `gitcomet_view.rs` next to `terminal_panel_height: px(TERMINAL_PANEL_DEFAULT_HEIGHT_PX)` (`:1959`). The toggle: when maximized, `terminal_panel_height = self.terminal_panel_height_before_maximize.take().unwrap_or(px(TERMINAL_PANEL_DEFAULT_HEIGHT_PX))` and clear the flag; otherwise stash the current height and set the flag. `cx.notify()` either way. While maximized, leave `terminal_panel_height` untouched — the panel is sized by `flex_1`, so the field is simply parked for the restore.

- [ ] **Step 4: Run the state tests and confirm they pass**

Run: `cargo test -p gitcomet-ui-gpui --lib maximizing_the_terminal_panel`
Expected: PASS

- [ ] **Step 5: Write the failing layout tests**

In `terminal_panel/tests/maximize.rs`. Both draw the view (`crate::test_support::refresh_and_draw(cx)`) and read `cx.debug_bounds("terminal_context_surface")` (`terminal_panel.rs:1084`).

```rust
#[gpui::test]
fn a_maximized_terminal_panel_grows_upward_without_changing_its_width(cx: &mut gpui::TestAppContext)
```
Insert a session, draw, record the `terminal_context_surface` bounds. Maximize, draw, record again. Assert `after.size.width == before.size.width` and `after.size.height > before.size.height` — the "up, not to the sides" requirement, pinned as a pair.

```rust
#[gpui::test]
fn a_maximized_panel_hides_the_resize_handle_and_the_main_content(cx: &mut gpui::TestAppContext)
```
Assert `cx.debug_bounds("terminal_panel_resize")` and `cx.debug_bounds("main_content_slot")` are both `Some` before maximizing and both `None` after.

- [ ] **Step 6: Run them and confirm they fail**

Run: `cargo test -p gitcomet-ui-gpui --lib a_maximized`
Expected: FAIL — heights equal (no render branch yet), and `debug_bounds("main_content_slot")` is `None` even before maximizing because that selector does not exist yet.

- [ ] **Step 7: Swap the panel's fixed height for `flex_1` when maximized**

In `render_terminal_panel`'s outer div (`terminal_panel.rs:1137-1156`), replace `.h(self.terminal_panel_height)` (`:1141`) with `.when(!maximized, |d| d.h(self.terminal_panel_height)).when(maximized, |d| d.flex_1())`, where `maximized` is read once at the top of the function. Keep `.min_h(px(TERMINAL_PANEL_MIN_HEIGHT_PX))`.

Apply the same branch to the two height wrappers in `reflog_panel.rs`: the single-panel path (`:131-150`, `.h(self.terminal_panel_height)`) and the tabbed path (`:152-172`, `.h(self.terminal_panel_height + tab_bar_height)`). In the tabbed path keep adding `tab_bar_height` to the `min_h`.

- [ ] **Step 8: Drop the main content and the handle from the layout when maximized**

In `splash.rs`, read `let maximized = self.terminal_panel_maximized;` before the panel bindings (`:747`). Gate the handle: `bottom_panel.is_some().then_some(!maximized).then(|| …)` (`:749-751`). In the `match (bottom_panel, bottom_panel_resize_handle)` block (`:858-873`), add a maximized arm that renders the panel alone in the column — `d.flex().flex_col().child(bottom_panel)` with no `main_content` child and no handle — ahead of the existing two arms. Add `.debug_selector(|| "main_content_slot".to_string())` to the `div().flex_1().min_h(px(0.0)).child(main_content)` wrapper (`:863-867`) so the omission is observable.

Add `.debug_selector(|| "terminal_panel_resize".to_string())` to the handle div in `terminal_panel_resize_handle` (`terminal_panel.rs:1454`, which currently carries only `.id(...)` and `.group(...)`) so its absence while maximized is observable.

- [ ] **Step 9: Run the layout tests and confirm they pass**

Run: `cargo test -p gitcomet-ui-gpui --lib a_maximized`
Expected: PASS

- [ ] **Step 10: Write the failing button test**

In `terminal_panel/tests/maximize.rs`:

```rust
#[gpui::test]
fn the_maximize_button_toggles_the_panel_and_swaps_its_icon(cx: &mut gpui::TestAppContext)
```
Insert a session, draw, and assert a control with id `terminal_maximize` is present and its painted icon quad is the maximize glyph. Activate it via the existing paint/activation idiom used in `terminal_panel/tests/context_menu.rs`, draw, and assert `terminal_panel_maximized` is `true` and the painted icon is now the restore glyph. Use `use crate::test_support::painted_control_quads as paint;` for the icon assertion.

- [ ] **Step 11: Add the toggle button to the terminal header**

In the trailing icon group (`terminal_panel.rs:1320-1389`), insert a button **before** `terminal_close`, built with the existing `icon_btn` closure (`:1181`) and the same `.on_activate(false, controls::ControlActivation::Action, cx.listener(…))` shape as its neighbours:

```rust
icon_btn(
    "terminal_maximize",
    if maximized { "icons/generic_restore.svg" } else { "icons/generic_maximize.svg" },
    if maximized { "Restore panel height" } else { "Maximize terminal" },
    false,
)
```

The listener calls `this.toggle_terminal_panel_maximized(cx)` and `cx.stop_propagation()`, matching `terminal_close` (`:1379-1385`). `maximized` is `self.terminal_panel_maximized` read before the closure so the `move` closure stays `Fn`. Both icon paths are `&'static str`, which is what `icon_btn` takes.

- [ ] **Step 12: Run the button test and confirm it passes**

Run: `cargo test -p gitcomet-ui-gpui --lib the_maximize_button`
Expected: PASS

- [ ] **Step 13: Run the full crate suite and confirm nothing regressed**

Run: `cargo test -p gitcomet-ui-gpui --lib`
Expected: PASS. `comfortable_bottom_panel_tabs_grow_with_the_density` (`lifecycle.rs:300`) and the `reflog_panel` layout tests are the ones most likely to notice the height branch.

- [ ] **Step 14: Lint, format, commit**

```bash
cargo fmt
cargo clippy --workspace --no-default-features --features gix -- -D warnings
cargo test --workspace --no-default-features --features gix
git add -A && git commit -m "Add in-app maximize and restore for the terminal panel"
```

---

## Final gate

Run in order and require all three clean:

```bash
cargo fmt --check
cargo clippy --workspace --no-default-features --features gix -- -D warnings
cargo test --workspace --no-default-features --features gix
```

Then confirm by hand in a running app:

1. Start a long-running command (`tail -f` on a log), close the panel with the whole-panel `×`, reopen it from the action-bar Terminal button or ctrl-\`, and see the same shell with the output it produced while hidden.
2. Open `vim`, close the panel, wait a few seconds, reopen. Vim must come back at the size it last knew and redraw cleanly on the next maximize or drag — this is the "hidden terminals keep their last grid size" claim, which has no automated test because nothing in the code changes to break it.
3. Maximize: the panel covers the commit list and diff, while the sidebar and details pane keep their widths. Restore: the panel returns to its pre-maximize height and the diff is where you left it.
4. With a session hidden, close the repo tab: the shell must die (`ps` / `htop` on the shell PID).