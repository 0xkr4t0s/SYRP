// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B5: `NamedPipe` between two threads and between two processes.
#![cfg(any(unix, windows))]

use std::io::ErrorKind;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use syrp_net::{Diagnostic, NamedPipe, NetError};

const LONG: Option<Duration> = Some(Duration::from_secs(10));
const CHILD_ENV: &str = "SYRP_NET_PIPE_CHILD";

/// A pipe name no other test or run uses.
fn unique(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!("syrp-test-{tag}-{}-{nanos}", std::process::id())
}

#[test]
fn b5_two_threads_exchange_both_ways() {
    let name = unique("threads");
    let server = NamedPipe::create_new_pipe(&name, true).expect("create");
    thread::scope(|s| {
        s.spawn(|| {
            let client = NamedPipe::open_existing(&name).expect("open");
            assert_eq!(client.name(), name);
            assert_eq!(client.write(b"ping", LONG).expect("write"), 4);
            let mut buf = [0u8; 4];
            assert_eq!(client.read(&mut buf, LONG).expect("read"), 4);
            assert_eq!(&buf, b"pong");
        });
        let mut buf = [0u8; 4];
        assert_eq!(server.read(&mut buf, LONG).expect("read"), 4);
        assert_eq!(&buf, b"ping");
        assert_eq!(server.write(b"pong", LONG).expect("write"), 4);
    });
}

#[test]
fn b5_large_transfer_arrives_in_order() {
    let name = unique("large");
    let server = NamedPipe::create_new_pipe(&name, true).expect("create");
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 253) as u8).collect();
    thread::scope(|s| {
        s.spawn(|| {
            let client = NamedPipe::open_existing(&name).expect("open");
            assert_eq!(client.write(&data, LONG).expect("write"), data.len());
            // Stay open until the reader has everything (Windows drops unread data).
            client.read(&mut [0u8; 1], LONG).expect("ack");
        });
        let mut got = vec![0u8; data.len()];
        assert_eq!(server.read(&mut got, LONG).expect("read"), data.len());
        assert!(got == data);
        server.write(b"k", LONG).expect("ack");
    });
}

#[test]
fn b5_create_and_open_rules() {
    let name = unique("rules");
    let missing = NamedPipe::open_existing(&name).unwrap_err();
    assert!(
        matches!(
            missing,
            NetError::Pipe {
                kind: ErrorKind::NotFound,
                ..
            }
        ),
        "{missing:?}"
    );
    assert_eq!(missing.code().as_str(), "SYRP-NET-0009");
    let first = NamedPipe::create_new_pipe(&name, true).expect("create");
    let again = NamedPipe::create_new_pipe(&name, true).unwrap_err();
    assert!(
        matches!(
            again,
            NetError::Pipe {
                kind: ErrorKind::AlreadyExists,
                ..
            }
        ),
        "{again:?}"
    );
    let reused = NamedPipe::create_new_pipe(&name, false);
    assert!(reused.is_ok(), "{reused:?}");
    for bad in [
        "",
        ".hidden",
        "a/b",
        "a\\b",
        "sp ace",
        "é",
        &"x".repeat(201),
    ] {
        let err = NamedPipe::create_new_pipe(bad, false).unwrap_err();
        assert_eq!(err.code().as_str(), "SYRP-NET-0008", "{bad:?}");
        assert_eq!(
            NamedPipe::open_existing(bad).unwrap_err().code().as_str(),
            "SYRP-NET-0008"
        );
    }
    assert!(first.is_open());
    first.close();
    assert!(!first.is_open());
    assert_eq!(first.write(b"x", None).unwrap_err(), NetError::Closed);
}

#[test]
fn b5_read_times_out_with_what_arrived() {
    let name = unique("timeout");
    let server = NamedPipe::create_new_pipe(&name, true).expect("create");
    let start = Instant::now();
    let n = server
        .read(&mut [0u8; 4], Some(Duration::from_millis(100)))
        .expect("read");
    assert_eq!(n, 0);
    assert!(start.elapsed() >= Duration::from_millis(90));
    thread::scope(|s| {
        s.spawn(|| {
            let client = NamedPipe::open_existing(&name).expect("open");
            client.write(b"ab", LONG).expect("write");
            client.read(&mut [0u8; 1], LONG).expect("ack");
        });
        let mut buf = [0u8; 4];
        assert_eq!(
            server
                .read(&mut buf, Some(Duration::from_secs(1)))
                .expect("read"),
            2
        );
        assert_eq!(&buf[..2], b"ab");
        server.write(b"k", LONG).expect("ack");
    });
}

#[test]
fn b5_close_from_another_thread_unblocks_read_and_write() {
    // Nothing is ever written to `idle`, and nobody reads what `client` writes to `full`.
    let idle = NamedPipe::create_new_pipe(&unique("close-read"), true).expect("create");
    let full_name = unique("close-write");
    let _full = NamedPipe::create_new_pipe(&full_name, true).expect("create");
    let client = NamedPipe::open_existing(&full_name).expect("open");
    thread::scope(|s| {
        let reader = s.spawn(|| idle.read(&mut [0u8; 8], None));
        // A large write fills the pipe and then waits for room.
        let writer = s.spawn(|| client.write(&vec![0u8; 4 << 20], None));
        thread::sleep(Duration::from_millis(200));
        let start = Instant::now();
        idle.close();
        client.close();
        assert_eq!(reader.join().expect("join"), Err(NetError::Closed));
        assert_eq!(writer.join().expect("join"), Err(NetError::Closed));
        assert!(start.elapsed() < Duration::from_secs(2));
    });
}

#[test]
fn b5_child_process_is_the_other_end() {
    let name = unique("child");
    let server = NamedPipe::create_new_pipe(&name, true).expect("create");
    let exe = std::env::current_exe().expect("test binary");
    let mut child = Command::new(exe)
        .args(["b5_child_end_helper", "--exact", "--nocapture"])
        .env(CHILD_ENV, &name)
        .spawn()
        .expect("spawn child");
    server.write(b"hello", LONG).expect("write");
    let mut buf = [0u8; 5];
    assert_eq!(server.read(&mut buf, LONG).expect("read"), 5);
    assert_eq!(&buf, b"HELLO");
    server.write(b"k", LONG).expect("ack");
    assert!(child.wait().expect("child exit").success());
}

/// Helper mode for `b5_child_process_is_the_other_end`: a no-op unless the parent set
/// the pipe name in the environment.
#[test]
fn b5_child_end_helper() {
    let Ok(name) = std::env::var(CHILD_ENV) else {
        return;
    };
    let pipe = NamedPipe::open_existing(&name).expect("child open");
    let mut buf = [0u8; 5];
    assert_eq!(pipe.read(&mut buf, LONG).expect("child read"), 5);
    pipe.write(&buf.to_ascii_uppercase(), LONG)
        .expect("child write");
    pipe.read(&mut [0u8; 1], LONG).expect("child ack");
}
