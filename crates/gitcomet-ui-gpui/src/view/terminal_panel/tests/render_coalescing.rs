use super::super::*;
use super::support::*;

/// The coalescer is pure state, so the burst/tick contract is checked directly:
/// one arm per burst, one flush per arm, and trailing output never starves.
#[test]
fn a_burst_of_grid_updates_coalesces_into_one_repaint_per_tick() {
    let coalescer = TerminalRenderCoalescer::default();

    assert!(
        coalescer.mark_dirty(),
        "the first mark of a burst arms the coalescing tick"
    );
    for _ in 0..50 {
        assert!(
            !coalescer.mark_dirty(),
            "a tick that is already in flight absorbs the rest of the burst"
        );
    }

    assert!(
        coalescer.claim_burst(),
        "the tick repaints the burst exactly once"
    );
    assert!(
        !coalescer.claim_burst(),
        "a second tick for the same burst must not repaint again"
    );

    assert!(
        coalescer.mark_dirty(),
        "output after a flush arms a fresh tick, so a trailing burst still lands"
    );
    assert!(coalescer.claim_burst(), "and that burst flushes");
}

#[gpui::test]
fn terminal_wakeups_notify_the_viewport_once_per_frame(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    let (events_tx, viewport) = cx.update(|_window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(70, Some(events_rx))], 0, cx),
            );
            let viewport = this
                .terminal_sessions
                .get(&repo_id)
                .and_then(|session| session.active_instance())
                .expect("a terminal tab")
                .viewport
                .clone();
            this.spawn_terminal_event_task(repo_id, 70, cx);
            (events_tx, viewport)
        })
    });

    let notified = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let _subscription = cx.update(|_window, app| {
        let notified = std::rc::Rc::clone(&notified);
        app.observe(&viewport, move |_, _| notified.set(notified.get() + 1))
    });

    // A `find / -type f` burst: 25 read batches, 25 wakeups.
    for _ in 0..25 {
        events_tx
            .try_send(TerminalBackendEvent::Wakeup)
            .expect("send terminal wakeup");
    }
    cx.run_until_parked();
    assert_eq!(
        notified.get(),
        0,
        "grid content waits for the coalescing tick instead of notifying per batch"
    );

    cx.executor()
        .advance_clock(TERMINAL_RENDER_COALESCE_INTERVAL + Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(
        notified.get(),
        1,
        "25 wakeups must cost exactly one viewport repaint"
    );

    // Trailing output after the burst still flushes on its own tick.
    events_tx
        .try_send(TerminalBackendEvent::Wakeup)
        .expect("send terminal wakeup");
    cx.run_until_parked();
    assert_eq!(notified.get(), 1, "the trailing wakeup waits for its tick");
    cx.executor()
        .advance_clock(TERMINAL_RENDER_COALESCE_INTERVAL + Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(notified.get(), 2, "and it lands one repaint, not none");

    cx.update(|_window, app| {
        let epoch = viewport.read(app).content_epoch;
        assert_eq!(epoch, 1 + 26, "every wakeup still advances the grid epoch");
    });
}

#[gpui::test]
fn terminal_title_updates_still_notify_immediately(cx: &mut gpui::TestAppContext) {
    let _visual_guard = crate::test_support::lock_visual_test();
    let (view, repo_id, cx) = test_root_view_with_active_repo(cx);

    let events_tx = cx.update(|_window, app| {
        view.update(app, |this, cx| {
            let (events_tx, events_rx) = smol::channel::unbounded();
            this.terminal_sessions.insert(
                repo_id,
                test_terminal_session(vec![(71, Some(events_rx))], 0, cx),
            );
            this.spawn_terminal_event_task(repo_id, 71, cx);
            events_tx
        })
    });

    events_tx
        .try_send(TerminalBackendEvent::Title("cargo test".to_string()))
        .expect("send terminal title");
    cx.run_until_parked();

    // No clock advance: a tab title is rare and correctness-critical, so it is
    // never held back by the render coalescer.
    assert_eq!(
        cx.update(|_window, app| view
            .read(app)
            .terminal_sessions
            .get(&repo_id)
            .and_then(|session| session.active_instance())
            .expect("a terminal tab")
            .title
            .clone()),
        "cargo test"
    );
}
