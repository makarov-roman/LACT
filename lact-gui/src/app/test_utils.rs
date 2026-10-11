use gtk::{glib, prelude::*};
use std::time::Duration;

pub fn find_widget<W: IsA<gtk::Widget>>(widget: &gtk::Widget) -> Option<W> {
    if let Ok(widget) = widget.clone().downcast::<W>() {
        return Some(widget);
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(widget) = find_widget(&widget) {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

pub fn receive<T>(context: &glib::MainContext, receiver: &relm4::Receiver<T>) -> T {
    context
        .block_on(glib::future_with_timeout(
            Duration::from_secs(5),
            receiver.recv(),
        ))
        .expect("timed out waiting for output")
        .expect("output channel closed")
}

pub fn assert_silent<T>(context: &glib::MainContext, receiver: &relm4::Receiver<T>) {
    assert!(
        context
            .block_on(glib::future_with_timeout(
                Duration::from_millis(50),
                receiver.recv()
            ))
            .is_err(),
        "unexpected output"
    );
}

pub fn wait_for(context: &glib::MainContext, condition: impl Fn() -> bool) {
    context
        .block_on(glib::future_with_timeout(Duration::from_secs(5), async {
            while !condition() {
                glib::timeout_future(Duration::from_millis(1)).await;
            }
        }))
        .unwrap();
}
