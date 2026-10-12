use super::super::*;
use super::support::*;

fn show_confirmation(
    view: &Entity<GitCometView>,
    action: TerminalShutdownAction,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            // The terminal backend is simulated; exercise the real dialog and
            // click handler after the running-command check has queued it.
            this.queue_terminal_shutdown_prompt(
                action,
                TerminalShutdownSummary {
                    terminal_count: 1,
                    running_command_count: 1,
                    repo_names: vec!["terminal-exit-test".to_string()],
                },
                cx,
            );
        });
    });
    cx.run_until_parked();
    crate::test_support::refresh_and_draw(cx);
}

fn click_dialog_button(selector: &'static str, cx: &mut gpui::VisualTestContext) {
    let button = cx.debug_bounds(selector).expect("terminal shutdown button");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
}

/// The store owns which repositories are open, and the view drops a repository's
/// terminal session when the snapshot it applies no longer lists it. Wait for
/// the store worker to land the close, then push the resulting snapshot the way
/// the app layer does in production.
fn wait_for_repo_to_close(view: &Entity<GitCometView>, cx: &mut gpui::VisualTestContext) {
    let store = cx.update(|_window, app| view.read(app).store.clone());
    for _ in 0..600 {
        if store.snapshot().repos.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        store.snapshot().repos.is_empty(),
        "the store never dropped the repository"
    );
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            crate::view::test_support::sync_store_snapshot(this, cx);
        })
    });
    cx.run_until_parked();
}

/// `test_root_view_with_active_repo` only sets the view's copy of the state.
/// Seed the store with the same snapshot so a close below removes a repository
/// the store actually has.
fn seed_store_with_view_state(view: &Entity<GitCometView>, cx: &mut gpui::VisualTestContext) {
    cx.update(|_window, app| {
        view.update(app, |this, _cx| {
            let state = this.state.clone();
            this.store.replace_snapshot_for_test(state);
        })
    });
}

#[gpui::test]
fn terminal_shutdown_confirm_click_closes_only_the_selected_tab(cx: &mut gpui::TestAppContext) {
    let _guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(10, None), (20, None)], 0, cx),
            );
        });
    });
    show_confirmation(
        &view,
        TerminalShutdownAction::CloseTerminalTab {
            repo_id,
            session_seq: 10,
        },
        cx,
    );

    click_dialog_button("terminal_shutdown_confirm", cx);

    cx.update(|_window, app| {
        let root = view.read(app);
        let session = root.terminal_sessions.get(&repo_id).unwrap();
        assert_eq!(session.instances.len(), 1);
        assert_eq!(session.instances[0].session_seq, 20);
        assert_eq!(app.windows().len(), 1);
        assert!(!root.popover_host.read(app).is_open());
    });
}

#[gpui::test]
fn terminal_shutdown_confirm_click_closes_the_panel(cx: &mut gpui::TestAppContext) {
    let _guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            this.terminal_sessions
                .insert(repo_id, test_terminal_session(vec![(10, None)], 0, cx));
        });
    });
    seed_store_with_view_state(&view, cx);
    // Repointed. The whole-panel `x` now hides and asks nothing, so the only
    // shutdown prompt that still ends a whole repository's session is the one
    // closing a repository raises. Same dialog, same confirm handler.
    show_confirmation(&view, TerminalShutdownAction::CloseRepo { repo_id }, cx);

    click_dialog_button("terminal_shutdown_confirm", cx);
    wait_for_repo_to_close(&view, cx);

    cx.update(|_window, app| {
        let root = view.read(app);
        assert!(!root.terminal_sessions.contains_key(&repo_id));
        assert_eq!(app.windows().len(), 1);
        assert!(!root.popover_host.read(app).is_open());
    });
}

#[gpui::test]
fn terminal_shutdown_confirm_cancel_keeps_the_panel(cx: &mut gpui::TestAppContext) {
    let _guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            this.terminal_sessions
                .insert(repo_id, test_terminal_session(vec![(10, None)], 0, cx));
        });
    });
    show_confirmation(&view, TerminalShutdownAction::CloseRepo { repo_id }, cx);

    click_dialog_button("terminal_shutdown_cancel", cx);

    cx.update(|_window, app| {
        let root = view.read(app);
        assert_eq!(root.terminal_sessions[&repo_id].instances.len(), 1);
        assert!(root.pending_terminal_shutdown_prompt.is_none());
        assert!(!root.popover_host.read(app).is_open());
    });
}

#[gpui::test]
fn terminal_shutdown_confirm_click_closes_the_window(cx: &mut gpui::TestAppContext) {
    let _guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            this.terminal_sessions
                .insert(repo_id, test_terminal_session(vec![(10, None)], 0, cx));
        });
    });
    show_confirmation(&view, TerminalShutdownAction::CloseWindow, cx);

    click_dialog_button("terminal_shutdown_confirm", cx);

    cx.cx.update(|app| assert!(app.windows().is_empty()));
}

#[gpui::test]
fn terminal_shutdown_confirm_click_preserves_later_close_guards(cx: &mut gpui::TestAppContext) {
    let _guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            Arc::make_mut(&mut this.state).repos[0].push_in_flight = 1;
            this.terminal_sessions
                .insert(repo_id, test_terminal_session(vec![(10, None)], 0, cx));
        });
    });
    show_confirmation(&view, TerminalShutdownAction::CloseWindow, cx);

    click_dialog_button("terminal_shutdown_confirm", cx);
    crate::test_support::refresh_and_draw(cx);

    cx.update(|_window, app| {
        let root = view.read(app);
        assert_eq!(app.windows().len(), 1);
        assert_eq!(root.terminal_sessions[&repo_id].instances.len(), 1);
        assert_eq!(
            root.popover_host.read(app).close_guard_action(),
            Some(&TerminalShutdownAction::CloseWindow),
        );
    });
    assert!(cx.debug_bounds("close_guard_reasons").is_some());
}
