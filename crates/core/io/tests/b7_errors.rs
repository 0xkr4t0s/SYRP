//! B7 (the `IoError` part): codes and `std::io::Error` conversions.

use std::error::Error;
use std::io;

use syrp_error::Diagnostic;
use syrp_io::{IoError, IoErrorKind};

#[test]
fn b7_io_error_keeps_source_and_round_trips_through_std() {
    let err = IoError::from(io::Error::new(io::ErrorKind::PermissionDenied, "nope"));
    assert_eq!(err.kind(), IoErrorKind::Io);
    assert_eq!(err.code().as_str(), "SYRP-IO-0003");
    assert!(err.source().unwrap().to_string().contains("nope"));
    let std_err = io::Error::from(err);
    assert_eq!(std_err.kind(), io::ErrorKind::PermissionDenied);
    let back = IoError::from(std_err);
    assert_eq!(back.kind(), IoErrorKind::Io);
    assert!(
        back.source().is_some(),
        "the original source survives the round trip"
    );
}

#[test]
fn b7_kinds_map_to_std_kinds_and_codes() {
    let cases = [
        (
            IoErrorKind::EndOfStream,
            io::ErrorKind::UnexpectedEof,
            "SYRP-IO-0001",
        ),
        (
            IoErrorKind::InvalidData,
            io::ErrorKind::InvalidData,
            "SYRP-IO-0004",
        ),
        (
            IoErrorKind::OutOfMemory,
            io::ErrorKind::OutOfMemory,
            "SYRP-IO-0005",
        ),
        (IoErrorKind::Io, io::ErrorKind::Other, "SYRP-IO-0003"),
        (
            IoErrorKind::BufferFull,
            io::ErrorKind::StorageFull,
            "SYRP-IO-0002",
        ),
    ];
    for (kind, std_kind, code) in cases {
        let err = IoError::new(kind);
        assert_eq!(err.code().as_str(), code);
        assert_eq!(io::Error::from(err).kind(), std_kind);
    }
    let eof = IoError::from(io::Error::from(io::ErrorKind::UnexpectedEof));
    assert_eq!(eof.kind(), IoErrorKind::EndOfStream);
}
