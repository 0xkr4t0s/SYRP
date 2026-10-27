// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B6: error codes are catalogued and carry the OS error; unsupported targets fail cleanly.

use syrp_net::{Diagnostic, IpAddress, MacAddress};

/// The codes declared in `errors/net.toml`, in file order.
fn catalogue() -> Vec<String> {
    include_str!("../../../../errors/net.toml")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("code = \""))
        .map(|rest| rest.trim_end_matches('"').to_owned())
        .collect()
}

#[test]
fn b6_catalogue_numbers_codes_in_order() {
    let expected: Vec<String> = (1..=9).map(|n| format!("SYRP-NET-{n:04}")).collect();
    assert_eq!(catalogue(), expected);
}

#[cfg(any(unix, windows))]
#[test]
fn b6_errors_from_every_area_are_catalogued_and_carry_os_codes() {
    use syrp_net::{DatagramSocket, NamedPipe, StreamingSocket};
    let codes = catalogue();
    let pipe = NamedPipe::open_existing("syrp-test-b6-no-such-pipe").unwrap_err();
    assert!(pipe.os_error().is_some(), "{pipe:?}");
    let socket = DatagramSocket::new(false).expect("socket");
    socket.close();
    let errors = [
        IpAddress::parse("1.2.3").unwrap_err(),
        MacAddress::parse("00-11").unwrap_err(),
        StreamingSocket::connect("host.invalid", 9, None).unwrap_err(),
        StreamingSocket::create_listener(0, "127.0.0.1")
            .expect("listener")
            .write(b"x")
            .unwrap_err(),
        socket.bind_to_port(0, "").unwrap_err(),
        NamedPipe::create_new_pipe("bad/name", false).unwrap_err(),
        pipe,
    ];
    for err in errors {
        let code = err.code().as_str().to_owned();
        assert!(codes.contains(&code), "{code} is not in errors/net.toml");
        assert!(err.to_string().starts_with(&code), "{err}");
        assert!(err.hint().is_some(), "{code} has no hint");
    }
}

#[cfg(not(any(unix, windows)))]
#[test]
fn b6_constructors_return_unsupported_without_os_networking() {
    use syrp_net::{DatagramSocket, NamedPipe, NetError, SocketOptions, StreamingSocket};
    let unsupported = |e: NetError| matches!(e, NetError::Unsupported { .. });
    assert!(StreamingSocket::connect("127.0.0.1", 1, None).is_err_and(unsupported));
    assert!(StreamingSocket::create_listener(0, "").is_err_and(unsupported));
    let o = SocketOptions::new();
    assert!(StreamingSocket::create_listener_with_options(0, "", o).is_err_and(unsupported));
    assert!(DatagramSocket::new(false).is_err_and(unsupported));
    assert!(NamedPipe::create_new_pipe("p", true).is_err_and(unsupported));
    assert!(NamedPipe::open_existing("p").is_err_and(unsupported));
    assert!(IpAddress::all_addresses(true).is_err_and(unsupported));
    assert!(MacAddress::find_all_addresses().is_err_and(unsupported));
}
