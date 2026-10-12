use super::super::*;
use super::support::*;
use crate::test_support::refresh_and_draw;

#[gpui::test]
fn terminal_toolbar_fill_tracks_panel_lifetime_and_target(cx: &mut gpui::TestAppContext) {
    let _guard = crate::test_support::lock_visual_test();
    assert_terminal_toolbar_fill_tracks_panel_lifetime_and_target(cx);
}

struct StatusBarTerminal;

impl gitcomet_extension_api::Extension for StatusBarTerminal {
    fn id(&self) -> gitcomet_extension_api::ExtensionId {
        gitcomet_extension_api::ExtensionId::new("com.example.terminal-layout").unwrap()
    }

    fn register(&self, registrar: &mut gitcomet_extension_api::Registrar) {
        registrar.place_bar_item(
            gitcomet_extension_api::BuiltinBarItem::Terminal,
            gitcomet_extension_api::BarItemLocation::StatusBarEnd,
        );
    }
}

#[gpui::test]
fn relocated_terminal_toolbar_fill_tracks_panel_lifetime_and_target(cx: &mut gpui::TestAppContext) {
    let _guard = crate::test_support::lock_visual_test();
    cx.update(|app| {
        crate::view::extension_host::install(
            gitcomet_extension_api::Registry::build(vec![Box::new(StatusBarTerminal)]).unwrap(),
            app,
        );
    });
    assert_terminal_toolbar_fill_tracks_panel_lifetime_and_target(cx);
}

fn assert_terminal_toolbar_fill_tracks_panel_lifetime_and_target(cx: &mut gpui::TestAppContext) {
    use crate::test_support::painted_control_quads as paint;
    use crate::view::test_support::redraw;

    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    cx.update(|_, app| {
        view.update(app, |view, cx| {
            let state = Arc::clone(&view.state);
            crate::view::test_support::push_test_state(view, state, cx);
            view.terminal_preferences.terminal_button_target = TerminalButtonTarget::Embedded;
            view.sync_terminal_button_target(cx);
        })
    });
    cx.run_until_parked();
    redraw(cx);
    let closed = paint(cx, "terminal");
    cx.update(|_, app| {
        view.update(app, |view, cx| {
            view.terminal_sessions
                .insert(repo_id, test_terminal_session(vec![(10, None)], 0, cx));
            view.sync_terminal_indicator_views(cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    redraw(cx);
    assert_ne!(
        paint(cx, "terminal"),
        closed,
        "an open embedded terminal must highlight its toggle"
    );
    cx.update(|_, app| {
        view.update(app, |view, cx| {
            view.terminal_preferences.terminal_button_target = TerminalButtonTarget::External;
            view.sync_terminal_button_target(cx);
        })
    });
    cx.run_until_parked();
    redraw(cx);
    assert_eq!(
        paint(cx, "terminal"),
        closed,
        "the external terminal launcher is momentary"
    );
    cx.update(|window, app| {
        view.update(app, |view, cx| {
            view.terminal_preferences.terminal_button_target = TerminalButtonTarget::Embedded;
            view.sync_terminal_button_target(cx);
            view.close_terminal_tab(repo_id, 0, window, cx);
        })
    });
    cx.run_until_parked();
    redraw(cx);
    assert_eq!(
        paint(cx, "terminal"),
        closed,
        "closing the last tab must clear the toggle"
    );
}

#[gpui::test]
fn the_terminal_toggle_defaults_to_the_action_bar(cx: &mut gpui::TestAppContext) {
    let _guard = crate::test_support::lock_visual_test();
    let (view, _repo_id, cx) = test_root_view_with_active_repo(cx);
    cx.update(|_, app| {
        view.update(app, |view, cx| {
            let state = Arc::clone(&view.state);
            crate::view::test_support::push_test_state(view, state, cx);
        })
    });
    cx.run_until_parked();
    crate::test_support::refresh_and_draw(cx);

    let terminal = cx.debug_bounds("terminal").expect("the terminal toggle");
    let branch = cx.debug_bounds("create_branch").expect("the branch action");
    let stash = cx.debug_bounds("stash").expect("the stash action");
    let actions = cx
        .debug_bounds("right_action_group")
        .expect("the action bar's right group");
    assert!(
        actions.contains(&terminal.center()),
        "Terminal belongs in the action bar: {terminal:?} vs {actions:?}"
    );
    assert!(
        terminal.right() <= branch.left() && branch.right() <= stash.left(),
        "Terminal, Branch and Stash retain their original order"
    );
    assert!(
        (terminal.center().y - branch.center().y).abs() < gpui::px(1.0),
        "on the action bar's row: {terminal:?} vs {branch:?}"
    );
}

#[gpui::test]
fn shell_exit_closes_the_matching_tab_after_indices_shift(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    let events_tx = cx.update(|window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(10, None), (20, Some(events_rx)), (30, None)], 2, cx),
            );
            this.spawn_terminal_event_task(repo_id, 20, cx);

            // Shift the target from index 1 to index 0 before its delayed
            // exit arrives. An index captured at spawn would close seq 30.
            this.close_terminal_tab(repo_id, 0, window, cx);
            events_tx
        })
    });

    events_tx
        .try_send(TerminalBackendEvent::Exit)
        .expect("send terminal exit event");
    cx.run_until_parked();

    cx.update(|_window, app| {
        let session = view
            .read(app)
            .terminal_sessions
            .get(&repo_id)
            .expect("surviving terminal session");
        assert_eq!(
            session
                .instances
                .iter()
                .map(|instance| instance.session_seq)
                .collect::<Vec<_>>(),
            vec![30]
        );
        assert_eq!(session.active_index, 0);
    });
}

#[gpui::test]
fn shell_exit_of_the_last_tab_closes_the_terminal_panel(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    let events_tx = cx.update(|_window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(40, Some(events_rx))], 0, cx),
            );
            this.spawn_terminal_event_task(repo_id, 40, cx);
            events_tx
        })
    });

    events_tx
        .try_send(TerminalBackendEvent::Exit)
        .expect("send terminal exit event");
    cx.run_until_parked();

    cx.update(|window, app| {
        view.update(app, |this, cx| {
            assert!(!this.terminal_sessions.contains_key(&repo_id));
            let theme = this.theme;
            assert!(this.render_terminal_panel(theme, window, cx).is_none());
            assert!(!this.terminal_cursor_blink_active);
        });
    });
}

#[gpui::test]
fn stale_shell_exit_after_manual_close_does_not_close_another_tab(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    let events_tx = cx.update(|window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(50, Some(events_rx)), (60, None)], 1, cx),
            );
            this.spawn_terminal_event_task(repo_id, 50, cx);
            this.close_terminal_tab(repo_id, 0, window, cx);
            events_tx
        })
    });

    events_tx
        .try_send(TerminalBackendEvent::Exit)
        .expect("send stale terminal exit event");
    cx.run_until_parked();

    cx.update(|_window, app| {
        let session = view
            .read(app)
            .terminal_sessions
            .get(&repo_id)
            .expect("surviving terminal session");
        assert_eq!(session.instances.len(), 1);
        assert_eq!(session.instances[0].session_seq, 60);
        assert_eq!(session.active_index, 0);
    });
}

#[gpui::test]
fn shutdown_confirmation_for_an_exited_tab_does_not_close_its_sibling(
    cx: &mut gpui::TestAppContext,
) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    let (events_tx, prompt) = cx.update(|_window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(70, Some(events_rx)), (80, None)], 0, cx),
            );
            this.spawn_terminal_event_task(repo_id, 70, cx);
            let prompt = TerminalShutdownPrompt {
                action: TerminalShutdownAction::CloseTerminalTab {
                    repo_id,
                    session_seq: 70,
                },
                summary: TerminalShutdownSummary {
                    terminal_count: 1,
                    running_command_count: 1,
                    repo_names: vec!["terminal-exit-test".to_string()],
                },
            };
            (events_tx, prompt)
        })
    });

    // The target exits while its terminate-and-close prompt is still open.
    // Its sibling shifts into index 0, which an index-keyed prompt would
    // incorrectly terminate and remove when confirmed.
    events_tx
        .try_send(TerminalBackendEvent::Exit)
        .expect("send terminal exit event");
    cx.run_until_parked();

    cx.update(|window, app| {
        view.update(app, |this, cx| {
            this.confirm_terminal_shutdown(prompt, window, cx);
        });
    });

    cx.update(|_window, app| {
        let session = view
            .read(app)
            .terminal_sessions
            .get(&repo_id)
            .expect("sibling terminal session");
        assert_eq!(session.instances.len(), 1);
        assert_eq!(session.instances[0].session_seq, 80);
        assert_eq!(session.active_index, 0);
    });
}

/// Both bottom-panel strips and the terminal's own tabs: click targets that
/// used to sit at a fixed pixel size whatever the density.
#[gpui::test]
fn comfortable_bottom_panel_tabs_grow_with_the_density(cx: &mut gpui::TestAppContext) {
    use crate::appearance::{Appearance, UiDensity};
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            this.terminal_sessions
                .insert(repo_id, test_terminal_session(vec![(10, None)], 0, cx));
            this.open_reflog_panel(repo_id, cx);
            this.active_bottom_panel
                .insert(repo_id, crate::view::BottomPanelTab::Terminal);
        })
    });

    let selectors = [
        "terminal_tab-0",
        "terminal_tab_close-0",
        "bottom_panel_tab_terminal",
        "bottom_panel_tab_terminal_close",
        "bottom_panel_tab_reflog",
    ];
    let mut per_density: Vec<Vec<gpui::Pixels>> = Vec::new();

    for density in UiDensity::ALL {
        cx.update(|_window, app| {
            app.set_global(Appearance {
                density,
                ..Appearance::default()
            });
            view.update(app, |this, cx| this.notify_font_preferences_changed(cx));
        });
        cx.update(|window, app| {
            let _ = window.draw(app);
        });
        cx.run_until_parked();

        let mut height = |selector: &'static str| {
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("missing {selector} at {density:?} density"))
                .size
                .height
        };
        per_density.push(selectors.into_iter().map(&mut height).collect());

        // Both strips are built from `panel_tab`, so they must agree.
        assert_eq!(
            height("terminal_tab-0"),
            height("bottom_panel_tab_terminal")
        );
        assert_eq!(
            height("terminal_tab_close-0"),
            height("bottom_panel_tab_terminal_close")
        );
    }

    for (step, pair) in per_density.windows(2).enumerate() {
        for (ix, selector) in selectors.into_iter().enumerate() {
            assert!(
                pair[1][ix] > pair[0][ix],
                "{selector} must grow from {:?} to {:?}, stayed {:?}",
                UiDensity::ALL[step],
                UiDensity::ALL[step + 1],
                pair[1][ix]
            );
        }
    }
}

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
            assert!(this.render_bottom_panel(theme, window, cx).is_none());
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

#[gpui::test]
fn showing_a_hidden_terminal_panel_restores_the_same_session(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    cx.update(|window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(51, Some(events_rx))], 0, cx),
            );
            let viewport_before = this.terminal_sessions[&repo_id]
                .instance_by_seq(51)
                .expect("the session under test")
                .viewport
                .entity_id();
            this.hide_terminal_panel_for_repo(repo_id, cx);
            this.show_terminal_panel_for_repo(repo_id, window, cx);

            let session = this
                .terminal_sessions
                .get(&repo_id)
                .expect("shown session is the one that was hidden");
            assert!(!session.panel_hidden);
            let restored = session
                .instance_by_seq(51)
                .expect("the same tab, not a respawned one");
            assert_eq!(
                restored.viewport.entity_id(),
                viewport_before,
                "showing the panel must return the same viewport entity, so a respawn \
                 with a fresh shell cannot pass"
            );
            assert_eq!(
                this.active_bottom_panel.get(&repo_id),
                Some(&BottomPanelTab::Terminal)
            );
            drop(events_tx);
        })
    });

    // The panel renders again. Asserted through a real draw rather than by
    // calling `render_terminal_panel` directly: building that element tree
    // outside a frame leaks the viewport handle, which gpui's leak detector
    // reports as a leaked `TerminalViewportView`. `terminal_context_surface`
    // is the panel's own debug selector, so drawn bounds are the same claim.
    refresh_and_draw(cx);
    assert!(cx.debug_bounds("terminal_context_surface").is_some());
    assert!(cx.debug_bounds("terminal_tab-0").is_some());
}

#[gpui::test]
fn the_terminal_toggle_hides_a_visible_panel_and_reshows_a_hidden_one(
    cx: &mut gpui::TestAppContext,
) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    let before = cx.update(|_window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(57, Some(events_rx))], 0, cx),
            );
            drop(events_tx);
            this.terminal_sessions[&repo_id]
                .instances
                .iter()
                .map(|instance| instance.session_seq)
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(before, vec![57]);

    cx.update(|window, app| {
        view.update(app, |this, cx| {
            this.toggle_terminal_for_active_repo(window, cx);
        });
    });
    cx.update(|_window, app| {
        view.update(app, |this, _cx| {
            assert!(
                this.terminal_sessions.contains_key(&repo_id),
                "hiding must not drop the session"
            );
            assert!(this.terminal_sessions[&repo_id].panel_hidden);
        });
    });

    cx.update(|window, app| {
        view.update(app, |this, cx| {
            this.toggle_terminal_for_active_repo(window, cx);
        });
    });
    cx.update(|_window, app| {
        view.update(app, |this, _cx| {
            let session = &this.terminal_sessions[&repo_id];
            assert!(!session.panel_hidden);
            assert_eq!(
                session
                    .instances
                    .iter()
                    .map(|instance| instance.session_seq)
                    .collect::<Vec<_>>(),
                before,
                "the toggle must not respawn a shell"
            );
        });
    });
}

#[gpui::test]
fn closing_the_repo_still_kills_a_hidden_terminal_session(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(55, Some(events_rx))], 0, cx),
            );
            this.hide_terminal_panel_for_repo(repo_id, cx);
            drop(events_tx);
            // `test_root_view_with_active_repo` only set the view's copy of the
            // state. Seed the store with the same snapshot so closing the repo
            // below removes a repository the store actually has.
            let state = this.state.clone();
            this.store.replace_snapshot_for_test(state);
        })
    });

    // The route a repository tab's `x` takes. Nothing is running, so no prompt
    // is queued and the close goes straight to the store.
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            this.request_close_repos(vec![repo_id], None, cx)
        });
    });
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
    // The app layer pushes that snapshot to the view; `apply_state_snapshot`
    // is what takes the session with the repository.
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            crate::view::test_support::sync_store_snapshot(this, cx);
        })
    });
    cx.run_until_parked();

    cx.update(|_window, app| {
        assert!(
            !view.read(app).terminal_sessions.contains_key(&repo_id),
            "hiding the panel must not shield the session from closing the repo"
        );
    });
}

#[gpui::test]
fn hiding_the_terminal_panel_asks_no_terminate_confirmation(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    cx.update(|window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(56, Some(events_rx))], 0, cx),
            );
            this.hide_terminal_panel_for_repo(repo_id, cx);

            assert!(this.pending_terminal_shutdown_prompt.is_none());
            let theme = this.theme;
            assert!(this.render_bottom_panel(theme, window, cx).is_none());
            drop(events_tx);
        })
    });
}

#[gpui::test]
fn closing_a_tab_still_kills_that_tab_and_leaves_the_panel_visible(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    cx.update(|window, app| {
        view.update(app, |this, cx| {
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(52, None), (53, None)], 1, cx),
            );
            this.close_terminal_tab(repo_id, 1, window, cx);

            let session = this
                .terminal_sessions
                .get(&repo_id)
                .expect("closing one tab keeps the session");
            assert!(session.instance_by_seq(53).is_none());
            assert!(session.instance_by_seq(52).is_some());
            assert!(!session.panel_hidden);
        })
    });

    // The surviving tab still renders, through a draw rather than a direct
    // `render_terminal_panel` call; see
    // `showing_a_hidden_terminal_panel_restores_the_same_session`.
    refresh_and_draw(cx);
    assert!(cx.debug_bounds("terminal_context_surface").is_some());
    assert!(cx.debug_bounds("terminal_tab-0").is_some());
}
