// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B3: `StreamingSocket` and `SocketOptions` over loopback.
#![cfg(any(unix, windows))]

use std::thread;
use std::time::{Duration, Instant};

use syrp_net::{Diagnostic, NetError, Readiness, SocketOptions, StreamingSocket};

const SHORT: Option<Duration> = Some(Duration::from_millis(50));

/// A connected (client, server) pair on `host`, or `None` when `host` cannot be bound.
fn pair(host: &str, options: SocketOptions) -> Option<(StreamingSocket, StreamingSocket)> {
    let listener = StreamingSocket::create_listener_with_options(0, host, options).ok()?;
    let port = listener.bound_port().expect("listener port");
    let client = StreamingSocket::connect_with_options(host, port, SHORT.map(|t| t * 40), options)
        .expect("connect");
    let server = listener.wait_for_next_connection().expect("accept");
    Some((client, server))
}

fn exchange(host: &str) {
    let Some((client, server)) = pair(host, SocketOptions::new()) else {
        eprintln!("skipped: cannot listen on {host} on this runner");
        return;
    };
    assert_eq!(client.write(b"hello, server").expect("write"), 13);
    let mut buf = [0u8; 13];
    assert_eq!(server.read(&mut buf, true).expect("read"), 13);
    assert_eq!(&buf, b"hello, server");

    server.write(b"pong").expect("write back");
    let mut buf = [0u8; 64];
    let n = client.read(&mut buf, false).expect("read back");
    assert_eq!(&buf[..n], &b"pong"[..n]);
    assert!(n > 0);

    assert_eq!(client.host_name(), host);
    assert_eq!(client.port(), server.bound_port().expect("server port"));
    assert_eq!(server.port(), client.bound_port().expect("client port"));
    assert!(client.is_local() && server.is_local());
    assert!(client.is_connected() && server.is_connected());
}

#[test]
fn b3_connect_listen_and_exchange_over_ipv4() {
    exchange("127.0.0.1");
}

#[test]
fn b3_connect_listen_and_exchange_over_ipv6_when_available() {
    exchange("::1");
}

#[test]
fn b3_block_until_full_waits_for_every_byte() {
    let (client, server) = pair("127.0.0.1", SocketOptions::new()).expect("pair");
    let writer = thread::spawn(move || {
        for chunk in [&b"ab"[..], b"cd", b"ef"] {
            client.write(chunk).expect("write");
            thread::sleep(Duration::from_millis(20));
        }
        client
    });
    let mut buf = [0u8; 6];
    assert_eq!(server.read(&mut buf, true).expect("read"), 6);
    assert_eq!(&buf, b"abcdef");
    drop(writer.join().expect("writer"));
    assert_eq!(
        server.read(&mut buf, true).expect("read at end"),
        0,
        "peer closed"
    );
}

#[test]
fn b3_wait_until_ready_times_out_then_reports_ready() {
    let (client, server) = pair("127.0.0.1", SocketOptions::new()).expect("pair");
    assert_eq!(
        server.wait_until_ready(true, SHORT).expect("wait"),
        Readiness::TimedOut
    );
    assert_eq!(
        client.wait_until_ready(false, SHORT).expect("wait"),
        Readiness::Ready
    );
    client.write(b"x").expect("write");
    let r = server.wait_until_ready(true, Some(Duration::from_secs(5)));
    assert_eq!(r.expect("wait"), Readiness::Ready);
}

#[test]
fn b3_close_from_another_thread_unblocks_a_blocked_read() {
    let (_client, server) = pair("127.0.0.1", SocketOptions::new()).expect("pair");
    thread::scope(|s| {
        let reader = s.spawn(|| server.read(&mut [0u8; 8], false));
        thread::sleep(Duration::from_millis(100));
        let start = Instant::now();
        server.close();
        assert_eq!(reader.join().expect("reader"), Err(NetError::Closed));
        assert!(start.elapsed() < Duration::from_secs(2));
    });
    assert!(!server.is_connected());
    assert_eq!(server.bound_port(), None);
    assert_eq!(server.write(b"x"), Err(NetError::Closed));
}

#[test]
fn b3_close_from_another_thread_unblocks_wait_for_next_connection() {
    let listener = StreamingSocket::create_listener(0, "127.0.0.1").expect("listen");
    thread::scope(|s| {
        let waiter = s.spawn(|| listener.wait_for_next_connection());
        thread::sleep(Duration::from_millis(100));
        listener.close();
        let err = waiter.join().expect("waiter").expect_err("closed");
        assert_eq!(err.code().as_str(), "SYRP-NET-0007");
    });
}

#[test]
fn b3_socket_options_apply_on_creation_and_data_still_flows() {
    let options = SocketOptions::new()
        .with_receive_buffer_size(4096)
        .with_send_buffer_size(4096);
    let (client, server) = pair("127.0.0.1", options).expect("pair with options");
    let data: Vec<u8> = (0..256 * 1024).map(|i| (i % 251) as u8).collect();
    thread::scope(|s| {
        s.spawn(|| client.write(&data).expect("write"));
        let mut got = vec![0u8; data.len()];
        assert_eq!(server.read(&mut got, true).expect("read"), data.len());
        assert_eq!(got, data);
    });
}

#[test]
fn b3_errors_carry_codes() {
    // A port that was just free: binding it and dropping the listener frees it again.
    let port = StreamingSocket::create_listener(0, "127.0.0.1")
        .expect("listen")
        .bound_port()
        .expect("port");
    let err =
        StreamingSocket::connect("127.0.0.1", port, SHORT.map(|t| t * 40)).expect_err("refused");
    assert_eq!(err.code().as_str(), "SYRP-NET-0006");

    let err = StreamingSocket::create_listener(0, "not an address").expect_err("bad host");
    assert_eq!(err.code().as_str(), "SYRP-NET-0005");

    let listener = StreamingSocket::create_listener(0, "127.0.0.1").expect("listen");
    let err = listener
        .read(&mut [0u8; 1], false)
        .expect_err("listener read");
    assert!(
        matches!(
            err,
            NetError::Socket {
                operation: "read",
                ..
            }
        ),
        "{err}"
    );
}
