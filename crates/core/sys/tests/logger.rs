// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B6 tests. Only the first test touches the global current logger.

use std::io::Write;
use std::sync::{Arc, Mutex};
use syrp_sys::{Logger, WriterLogger, set_current_logger, write_to_log};

/// An `io::Write` whose bytes stay readable after the logger takes ownership of it.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Shared {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

#[test]
fn b6_current_logger_receives_messages_and_falls_back_to_stderr() {
    // With no logger, messages go to standard error and nothing panics.
    assert!(set_current_logger(None).is_none());
    write_to_log("b6: to stderr");

    let first = Shared::default();
    assert!(set_current_logger(Some(Box::new(WriterLogger::new(first.clone())))).is_none());
    write_to_log("one");
    let threads: Vec<_> = (0..4)
        .map(|i| std::thread::spawn(move || write_to_log(&format!("thread {i}"))))
        .collect();
    threads.into_iter().for_each(|t| t.join().unwrap());

    let second = Shared::default();
    let previous = set_current_logger(Some(Box::new(WriterLogger::new(second.clone()))));
    previous.expect("the first logger").log("direct");
    write_to_log("two");

    let text = first.text();
    assert!(
        text.starts_with("one\n") && text.ends_with("direct\n"),
        "{text}"
    );
    for i in 0..4 {
        assert!(text.contains(&format!("thread {i}\n")), "{text}");
    }
    assert_eq!(text.lines().count(), 6);
    assert_eq!(second.text(), "two\n");

    assert!(set_current_logger(None).is_some());
    write_to_log("b6: to stderr again");
    assert_eq!(second.text(), "two\n");
}

#[test]
fn b6_writer_logger_writes_lines_and_returns_its_writer() {
    let logger = WriterLogger::new(Vec::new());
    logger.log("a");
    logger.log("");
    logger.log("multi\nline");
    assert_eq!(logger.into_inner(), b"a\n\nmulti\nline\n");
}
