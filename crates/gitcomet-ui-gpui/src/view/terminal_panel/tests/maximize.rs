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
fn the_maximize_button_toggles_the_panel_and_swaps_its_icon(cx: &mut gpui::TestAppContext) {
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
