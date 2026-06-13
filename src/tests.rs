#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use crate::themes::{FontStyle, Typeface};
    use crate::ui::UI;

    fn test_ui() -> UI {
        let typeface = Typeface {
            font_name: "Test".to_string(),
            font_style: FontStyle::Regular,
            font_size: None,
        };
        UI::new(100, 100, typeface, 1.0)
    }

    #[test]
    fn ui_handle_runs_task_from_worker_thread() {
        let mut ui = test_ui();
        let handle = ui.handle();
        let ran = Arc::new(AtomicBool::new(false));

        let ran_clone = Arc::clone(&ran);
        std::thread::spawn(move || {
            handle.run_on_ui_thread(move |_ui| {
                ran_clone.store(true, Ordering::SeqCst);
            });
        })
        .join()
        .unwrap();

        // The queued task runs on the next tick and requests a redraw.
        assert!(ui.update());
        assert!(ran.load(Ordering::SeqCst));
        // No pending tasks left: nothing to do, no redraw.
        assert!(!ui.update());
    }

    #[test]
    fn ui_handle_task_queued_during_task_runs_next_tick() {
        let mut ui = test_ui();
        let handle = ui.handle();
        let count = Arc::new(AtomicUsize::new(0));

        let inner_handle = handle.clone();
        let count_outer = Arc::clone(&count);
        handle.run_on_ui_thread(move |_ui| {
            count_outer.fetch_add(1, Ordering::SeqCst);
            let count_inner = Arc::clone(&count_outer);
            inner_handle.run_on_ui_thread(move |_ui| {
                count_inner.fetch_add(1, Ordering::SeqCst);
            });
        });

        assert!(ui.update());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(ui.update());
        assert_eq!(count.load(Ordering::SeqCst), 2);
        assert!(!ui.update());
    }

    /// A SelectionChanged handler fired from ComboBox::update (inside the
    /// update tree-walk) must be able to call ui.get_view — the event is
    /// deferred until the walk releases its borrows (regression: panicked
    /// with "RefCell already mutably borrowed").
    #[test]
    fn combobox_selection_handler_can_use_get_view() {
        use std::cell::RefCell;
        use std::rc::Rc;
        use crate::containers::Frame;
        use crate::events::{EventData, EventType};
        use crate::traits::{Element, View};
        use crate::views::{ComboBox, Dimension, Label};

        let mut ui = test_ui();
        let root: Element = Rc::new(RefCell::new(Frame::new(
            crate::types::rect((0, 0), (100, 100)),
            Dimension::Max,
            Dimension::Max,
        )));
        ui.add_view(root);

        let mut combo = ComboBox::default();
        combo.set_any("id", "combo");
        combo.add_item("one");
        combo.add_item("two");
        let combo: Element = Rc::new(RefCell::new(combo));
        ui.add_view(combo);

        let mut label = Label::default();
        label.set_any("id", "label");
        let label: Element = Rc::new(RefCell::new(label));
        ui.add_view(label);

        let seen = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&seen);
        if let Some(view) = ui.get_view("combo") {
            view.borrow_mut().on_event(EventType::SelectionChanged, Box::new(move |ui, _view, data| {
                // The regression: this get_view call used to panic.
                assert!(ui.get_view("label").is_some());
                if let EventData::Selected(index) = data {
                    *sink.borrow_mut() = Some(*index);
                }
                true
            }));
        }

        // Simulate the dropdown writing the user's pick, as ComboDropdown does.
        if let Some(view) = ui.get_view("combo") {
            if let Some(combo) = view.borrow().downcast_ref::<ComboBox>() {
                combo.simulate_pending_selection(1);
            }
        }

        assert!(ui.update());
        assert_eq!(*seen.borrow(), Some(1));
    }

    #[test]
    fn on_close_fires_on_drop() {
        let fired = Arc::new(AtomicBool::new(false));
        let fired_clone = Arc::clone(&fired);
        {
            let mut ui = test_ui();
            ui.set_on_close(move || {
                fired_clone.store(true, Ordering::SeqCst);
            });
            assert!(!fired.load(Ordering::SeqCst));
        }
        assert!(fired.load(Ordering::SeqCst));
    }
}
