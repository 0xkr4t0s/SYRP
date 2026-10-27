// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B4: `DatagramSocket` over loopback (multicast is best effort).
#![cfg(any(unix, windows))]

use std::io::ErrorKind;
use std::thread;
use std::time::{Duration, Instant};

use syrp_net::{DatagramSocket, Diagnostic, IpAddress, NetError, Readiness, SocketOptions};

const SHORT: Option<Duration> = Some(Duration::from_millis(50));
const LONG: Option<Duration> = Some(Duration::from_secs(2));

/// A receiver bound to an OS-chosen loopback port, and that port.
fn receiver(options: SocketOptions) -> (DatagramSocket, u16) {
    let rx = DatagramSocket::new_with_options(false, options).expect("socket");
    rx.bind_to_port(0, "127.0.0.1").expect("bind");
    let port = rx.bound_port().expect("bound port");
    (rx, port)
}

#[test]
fn b4_bind_write_and_read_with_sender_address() {
    let (rx, port) = receiver(SocketOptions::new());
    let tx = DatagramSocket::new(false).expect("socket");
    assert_eq!(tx.bound_port(), None);
    assert_eq!(tx.write("127.0.0.1", port, b"hello").expect("write"), 5);
    let mut buf = [0u8; 32];
    let (n, from, from_port) = rx.read(&mut buf, true).expect("read");
    assert_eq!(&buf[..n], b"hello");
    assert_eq!(from, IpAddress::parse("127.0.0.1").expect("ip"));
    // Writing gave the sender a port, which the receiver saw and which can be read from.
    assert_eq!(Some(from_port), tx.bound_port());
    rx.write("127.0.0.1", from_port, b"back").expect("reply");
    let (n, _, reply_port) = tx.read(&mut buf, true).expect("read reply");
    assert_eq!((&buf[..n], reply_port), (&b"back"[..], port));
}

#[test]
fn b4_non_blocking_read_and_unbound_read() {
    let (rx, _) = receiver(SocketOptions::new());
    let (n, from, port) = rx.read(&mut [0u8; 8], false).expect("read");
    assert_eq!((n, from.is_null(), port), (0, true, 0));
    let unbound = DatagramSocket::new(false).expect("socket");
    match unbound.read(&mut [0u8; 8], false) {
        Err(NetError::Socket { kind, .. }) => assert_eq!(kind, ErrorKind::NotConnected),
        other => panic!("expected NotConnected, got {other:?}"),
    }
}

#[test]
fn b4_oversized_datagram_is_truncated_to_the_buffer() {
    let (rx, port) = receiver(SocketOptions::new());
    let tx = DatagramSocket::new(false).expect("socket");
    tx.write("127.0.0.1", port, &[7u8; 100]).expect("write");
    tx.write("127.0.0.1", port, b"next").expect("write");
    let mut small = [0u8; 10];
    assert_eq!(rx.read(&mut small, true).expect("read").0, 10);
    assert_eq!(small, [7u8; 10]);
    // The rest of the long datagram is discarded, not delivered with the next one.
    let mut buf = [0u8; 16];
    let (n, _, _) = rx.read(&mut buf, true).expect("read");
    assert_eq!(&buf[..n], b"next");
}

#[test]
fn b4_wait_until_ready_times_out_then_reports_ready() {
    let (rx, port) = receiver(SocketOptions::new());
    assert_eq!(
        rx.wait_until_ready(true, SHORT).expect("wait"),
        Readiness::TimedOut
    );
    DatagramSocket::new(false)
        .expect("socket")
        .write("127.0.0.1", port, b"x")
        .expect("write");
    assert_eq!(
        rx.wait_until_ready(true, LONG).expect("wait"),
        Readiness::Ready
    );
    assert_eq!(
        rx.wait_until_ready(false, SHORT).expect("wait"),
        Readiness::Ready
    );
}

#[test]
fn b4_close_from_another_thread_unblocks_a_blocked_read() {
    let (rx, _) = receiver(SocketOptions::new());
    thread::scope(|s| {
        let reader = s.spawn(|| rx.read(&mut [0u8; 8], true));
        thread::sleep(Duration::from_millis(100));
        let start = Instant::now();
        rx.close();
        assert_eq!(reader.join().expect("join"), Err(NetError::Closed));
        assert!(start.elapsed() < Duration::from_secs(2));
    });
    assert_eq!(rx.bound_port(), None);
    let err = rx.write("127.0.0.1", 9, b"x").unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-NET-0007");
}

#[test]
fn b4_broadcast_needs_enable_broadcasting() {
    let tx = DatagramSocket::new(false).expect("socket");
    let err = tx.write("255.255.255.255", 9, b"x").unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-NET-0006");
    tx.set_enable_broadcasting(true).expect("enable");
    match tx.write("255.255.255.255", 9, b"x") {
        Ok(n) => assert_eq!(n, 1),
        Err(e) => eprintln!("skipped: no broadcast route on this runner ({e})"),
    }
    assert!(DatagramSocket::new(true).is_ok());
}

#[test]
fn b4_multicast_join_send_and_leave_best_effort() {
    let group = IpAddress::parse("239.255.42.99").expect("group");
    let rx = DatagramSocket::new(false).expect("socket");
    rx.bind_to_port(0, "").expect("bind");
    let port = rx.bound_port().expect("port");
    if let Err(e) = rx.join_multicast(group) {
        eprintln!("skipped: cannot join a multicast group on this runner ({e})");
        return;
    }
    let tx = DatagramSocket::new(false).expect("socket");
    tx.set_multicast_loopback_enabled(true).expect("loopback");
    let sent = tx.write(&group.to_string(), port, b"mc");
    let ready = sent.is_ok() && rx.wait_until_ready(true, LONG).expect("wait") == Readiness::Ready;
    if ready {
        let mut buf = [0u8; 8];
        let (n, _, _) = rx.read(&mut buf, true).expect("read");
        assert_eq!(&buf[..n], b"mc");
    } else {
        eprintln!("skipped: no multicast route on this runner ({sent:?})");
    }
    rx.leave_multicast(group).expect("leave");
    let v6 = IpAddress::parse("ff02::1").expect("v6 group");
    assert!(matches!(
        rx.join_multicast(v6),
        Err(NetError::Socket {
            kind: ErrorKind::Unsupported,
            ..
        })
    ));
}

#[test]
fn b4_socket_options_apply_on_creation() {
    let options = SocketOptions::new()
        .with_receive_buffer_size(64 * 1024)
        .with_send_buffer_size(64 * 1024);
    let (rx, port) = receiver(options);
    let tx = DatagramSocket::new_with_options(false, options).expect("socket");
    tx.write("127.0.0.1", port, &[1u8; 1024]).expect("write");
    assert_eq!(rx.read(&mut [0u8; 2048], true).expect("read").0, 1024);
}

#[test]
fn b4_errors_carry_codes() {
    let s = DatagramSocket::new(false).expect("socket");
    let err = s.bind_to_port(0, "::1").unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-NET-0005");
    let err = s.write("host.invalid", 9, b"x").unwrap_err();
    assert_eq!(err.code().as_str(), "SYRP-NET-0005");
    s.close();
    assert_eq!(s.bind_to_port(0, "").unwrap_err(), NetError::Closed);
}
