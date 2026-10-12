use super::super::*;
use super::support::*;
use crate::test_support::refresh_and_draw;

/// A live session with the panel open, drawn through the real chrome so the
/// bottom panel, its resize handle and the main content slot are all laid out.
fn open_terminal_panel(
    view: &Entity<GitCometView>,
    repo_id: RepoId,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            crate::view::test_support::push_test_state(this, this.state.clone(), cx);
            this.terminal_sessions
                .insert(repo_id, test_terminal_session(vec![(90, None)], 0, cx));
            cx.notify();
        });
    });
    cx.run_until_parked();
    refresh_and_draw(cx);
}

/// The same, with the reflog panel open alongside the terminal, so the bottom
/// panel renders its tab strip and the tabbed wrapper in `render_bottom_panel`
/// is the branch on screen. Terminal stays the active tab, because that is what
/// keeps the header — and its maximize control — rendered.
fn open_terminal_and_reflog_panels(
    view: &Entity<GitCometView>,
    repo_id: RepoId,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            crate::view::test_support::push_test_state(this, this.state.clone(), cx);
            this.terminal_sessions
                .insert(repo_id, test_terminal_session(vec![(91, None)], 0, cx));
            this.open_reflog_panel(repo_id, cx);
            this.active_bottom_panel
                .insert(repo_id, BottomPanelTab::Terminal);
            cx.notify();
        });
    });
    cx.run_until_parked();
    refresh_and_draw(cx);
}

#[gpui::test]
fn maximizing_the_terminal_panel_and_restoring_returns_the_previous_height(
    cx: &mut gpui::TestAppContext,
) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, _repo_id, cx) = test_root_view_with_active_repo(cx);

    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            this.terminal_panel_height = px(300.0);
            this.toggle_terminal_panel_maximized(cx);
            assert!(
                this.terminal_panel_maximized,
                "the toggle maximizes the panel"
            );
            assert_eq!(
                this.terminal_panel_height_before_maximize,
                Some(px(300.0)),
                "the height the user dragged to is parked for the restore"
            );

            this.toggle_terminal_panel_maximized(cx);
            assert!(
                !this.terminal_panel_maximized,
                "the same control restores the panel"
            );
            assert!(
                this.terminal_panel_height_before_maximize.is_none(),
                "a parked height is consumed by the restore, so a second restore cannot reuse it"
            );
            assert_eq!(
                this.terminal_panel_height,
                px(300.0),
                "restoring returns the exact height from before maximizing"
            );
        })
    });
}

#[gpui::test]
fn restoring_a_maximized_panel_without_a_saved_height_uses_the_default(
    cx: &mut gpui::TestAppContext,
) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, _repo_id, cx) = test_root_view_with_active_repo(cx);

    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            // Maximized by hand, with nothing parked: the restore must land on
            // a sane height rather than whatever `terminal_panel_height`
            // happened to hold while the panel was sized by `flex_1`.
            this.terminal_panel_maximized = true;
            this.terminal_panel_height_before_maximize = None;
            this.toggle_terminal_panel_maximized(cx);
            assert!(
                !this.terminal_panel_maximized,
                "the toggle clears the flag whichever way the parked height went"
            );
            assert_eq!(
                this.terminal_panel_height,
                px(TERMINAL_PANEL_DEFAULT_HEIGHT_PX),
                "a restore with no saved height falls back to the default panel height"
            );
        })
    });
}

#[gpui::test]
fn a_maximized_terminal_panel_grows_upward_without_changing_its_width(
    cx: &mut gpui::TestAppContext,
) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    open_terminal_panel(&view, repo_id, cx);

    let before = cx
        .debug_bounds("terminal_context_surface")
        .expect("terminal surface");
    cx.update(|_window, app| {
        view.update(app, |this, cx| this.toggle_terminal_panel_maximized(cx));
    });
    refresh_and_draw(cx);
    let after = cx
        .debug_bounds("terminal_context_surface")
        .expect("terminal surface");

    // The panel grows over the commit list and diff, never sideways: the
    // sidebar and the details pane keep their widths, so the panel's own width
    // must not budge while its height does.
    assert_eq!(
        after.size.width, before.size.width,
        "maximizing must not change the panel's width ({before:?} -> {after:?})"
    );
    assert!(
        after.size.height > before.size.height,
        "maximizing must grow the panel upward ({before:?} -> {after:?})"
    );
    // "Upward", literally: the left edge stays where it was and the top edge
    // climbs, so the growth eats the commit list and diff and nothing beside.
    assert_eq!(
        after.origin.x, before.origin.x,
        "the panel must not slide sideways on its way up ({before:?} -> {after:?})"
    );
    assert!(
        after.origin.y < before.origin.y,
        "the panel's top edge must climb ({before:?} -> {after:?})"
    );

    // The restore direction, which is the half the user has to reach for:
    // byte-identical size, and the commit list, diff and drag handle back in
    // the layout.
    cx.update(|_window, app| {
        view.update(app, |this, cx| this.toggle_terminal_panel_maximized(cx));
    });
    refresh_and_draw(cx);
    let after_restore = cx
        .debug_bounds("terminal_context_surface")
        .expect("terminal surface");
    assert_eq!(
        after_restore.size, before.size,
        "restoring returns the panel to the exact size it had before maximizing \
         ({before:?} -> {after:?} -> {after_restore:?})"
    );
    assert!(
        cx.debug_bounds("main_content_slot").is_some(),
        "the commit list and diff come back when the panel is restored"
    );
    assert!(
        cx.debug_bounds("terminal_panel_resize").is_some(),
        "the drag handle comes back when the panel is restored"
    );
}

#[gpui::test]
fn a_maximized_panel_hides_the_resize_handle_and_the_main_content(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    open_terminal_panel(&view, repo_id, cx);

    assert!(
        cx.debug_bounds("terminal_panel_resize").is_some(),
        "the drag handle is on screen while the panel has a height of its own"
    );
    assert!(
        cx.debug_bounds("main_content_slot").is_some(),
        "the commit list and diff are on screen while the panel is docked"
    );

    cx.update(|_window, app| {
        view.update(app, |this, cx| this.toggle_terminal_panel_maximized(cx));
    });
    refresh_and_draw(cx);

    assert!(
        cx.debug_bounds("terminal_panel_resize").is_none(),
        "a maximized panel is not draggable, so the handle leaves the layout"
    );
    assert!(
        cx.debug_bounds("main_content_slot").is_none(),
        "a maximized panel covers the main content, which must leave the layout"
    );
}

#[gpui::test]
fn the_maximize_button_is_present_and_toggles_the_panel(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    open_terminal_panel(&view, repo_id, cx);

    let button = cx
        .debug_bounds("terminal_maximize")
        .expect("the header's maximize control");

    // The glyph/tooltip swap is pinned on the one helper the button reads both
    // from. `painted_control_quads` hands back fill/border quad pairs only, so
    // it cannot tell one SVG glyph from another and no paint assertion can
    // observe which icon actually drew.
    assert_eq!(
        terminal_panel_maximize_control(false),
        ("icons/generic_maximize.svg", "Maximize terminal")
    );
    assert_eq!(
        terminal_panel_maximize_control(true),
        ("icons/generic_restore.svg", "Restore panel height")
    );

    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    refresh_and_draw(cx);
    assert!(
        cx.update(|_window, app| view.read(app).terminal_panel_maximized),
        "clicking the control maximizes the panel"
    );

    let button = cx
        .debug_bounds("terminal_maximize")
        .expect("the header's restore control");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    refresh_and_draw(cx);
    assert!(
        !cx.update(|_window, app| view.read(app).terminal_panel_maximized),
        "the same control restores the panel"
    );
    assert_eq!(
        cx.update(|_window, app| view.read(app).terminal_panel_height),
        px(TERMINAL_PANEL_DEFAULT_HEIGHT_PX),
        "restoring through the control returns the height from before maximizing"
    );
}

#[gpui::test]
fn hiding_a_maximized_panel_restores_the_height_it_parked(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    // The reflog panel stays open, so the bottom panel survives the hide and the
    // main content only comes back if the panel also stopped being maximized.
    open_terminal_and_reflog_panels(&view, repo_id, cx);

    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            this.terminal_panel_height = px(300.0);
            this.toggle_terminal_panel_maximized(cx);
        });
    });
    refresh_and_draw(cx);
    assert!(
        cx.debug_bounds("main_content_slot").is_none(),
        "a maximized panel has the commit list and diff to itself"
    );

    // Hiding the terminal takes the header — and with it the only maximize
    // control — off screen, so the panel has to come back down by itself.
    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            this.hide_terminal_panel_for_repo(repo_id, cx)
        });
    });
    refresh_and_draw(cx);

    cx.update(|_window, app| {
        let this = view.read(app);
        assert!(
            !this.terminal_panel_maximized,
            "hiding a maximized panel un-maximizes it, because nothing left on \
             screen could restore it"
        );
        assert!(
            this.terminal_panel_height_before_maximize.is_none(),
            "and the parked height is consumed rather than left behind"
        );
        assert_eq!(
            this.terminal_panel_height,
            px(300.0),
            "the panel is back at the height it had before maximizing"
        );
    });
    assert!(
        cx.debug_bounds("main_content_slot").is_some(),
        "the commit list and diff are back in the layout, with no maximized \
         panel left to cover them"
    );
}

#[gpui::test]
fn a_maximized_bottom_panel_grows_the_reflog_tab_alongside_the_terminal(
    cx: &mut gpui::TestAppContext,
) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    open_terminal_and_reflog_panels(&view, repo_id, cx);

    let before_surface = cx
        .debug_bounds("terminal_context_surface")
        .expect("terminal surface");
    let before_tabs = cx
        .debug_bounds("bottom_panel_tab_reflog")
        .expect("the bottom panel's reflog tab");

    cx.update(|_window, app| {
        view.update(app, |this, cx| this.toggle_terminal_panel_maximized(cx));
    });
    refresh_and_draw(cx);

    let after_surface = cx
        .debug_bounds("terminal_context_surface")
        .expect("terminal surface");
    let after_tabs = cx
        .debug_bounds("bottom_panel_tab_reflog")
        .expect("the bottom panel's reflog tab");

    // With two panels open, `render_bottom_panel`'s tabbed wrapper is what sizes
    // the strip plus the showing panel, so a strip that climbs and content that
    // grows can only come from that wrapper going `flex_1` — the reflog
    // maximizes alongside, it is not left behind at its docked height.
    assert!(
        after_surface.size.height > before_surface.size.height,
        "the panel content grows with the tabbed wrapper \
         ({before_surface:?} -> {after_surface:?})"
    );
    assert!(
        after_tabs.origin.y < before_tabs.origin.y,
        "the whole bottom panel, tab strip included, grows upward \
         ({before_tabs:?} -> {after_tabs:?})"
    );
    assert_eq!(
        after_surface.size.width, before_surface.size.width,
        "and still without changing width ({before_surface:?} -> {after_surface:?})"
    );
}

#[gpui::test]
fn closing_the_terminal_tab_from_the_bottom_panel_strip_hides_the_panel(
    cx: &mut gpui::TestAppContext,
) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);
    open_terminal_and_reflog_panels(&view, repo_id, cx);

    // The tab strip's `x` is the only route through the bottom panel provider,
    // and it has to stow the panel exactly like the header's `x` does: the
    // session keeps running and the terminal button brings it back.
    let close = cx
        .debug_bounds("bottom_panel_tab_terminal_close")
        .expect("the bottom panel tab strip's close affordance for the terminal tab");
    cx.simulate_click(close.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    refresh_and_draw(cx);

    cx.update(|_window, app| {
        view.update(app, |this, cx| {
            let session = this
                .terminal_sessions
                .get(&repo_id)
                .expect("closing the terminal tab hides the panel, it does not kill the session");
            assert!(session.panel_hidden, "the panel is stowed");
            assert!(
                session.instance_by_seq(91).is_some(),
                "the tab and its shell are still there to be brought back"
            );
            assert!(
                this.reflog_panel_is_open(repo_id, cx),
                "the tab strip's close closes the tab it stands for, and nothing else"
            );
        });
    });
    assert!(
        cx.debug_bounds("terminal_context_surface").is_none(),
        "the panel must actually go away"
    );
}
