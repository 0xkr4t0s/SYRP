//! B1: the `InputStream` trait and the `StdInputStream` adapter.

use std::io::{Cursor, Read};

use syrp_error::Diagnostic;
use syrp_io::{InputStream, IoErrorKind, MemoryBlock, StdInputStream};

fn stream(bytes: &[u8]) -> StdInputStream<Cursor<Vec<u8>>> {
    StdInputStream::new(Cursor::new(bytes.to_vec())).unwrap()
}

#[test]
fn b1_length_position_and_seeking() {
    let mut s = stream(b"abcdef");
    assert_eq!(s.total_length(), Some(6));
    assert_eq!(s.position(), 0);
    s.set_position(4).unwrap();
    assert_eq!(s.read_u8().unwrap(), b'e');
    assert_eq!(s.position(), 5);
    assert_eq!(
        s.total_length(),
        Some(6),
        "querying the length keeps the position"
    );
    assert_eq!(s.read_u8().unwrap(), b'f');
    assert!(s.is_exhausted());
    s.set_position(0).unwrap();
    assert!(!s.is_exhausted());
    assert_eq!(s.skip_next_bytes(2).unwrap(), 2);
    assert_eq!(s.read_u8().unwrap(), b'c');
    assert_eq!(s.skip_next_bytes(100).unwrap(), 3);
    assert!(s.is_exhausted());
}

#[test]
fn b1_typed_reads_in_both_byte_orders() {
    let mut s = stream(&[
        1, 0, 0x12, 0x34, 0xFE, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0x80,
    ]);
    assert!(s.read_bool().unwrap());
    assert!(!s.read_bool().unwrap());
    assert_eq!(s.read_i16_be().unwrap(), 0x1234);
    assert_eq!(s.read_i32_le().unwrap(), -2);
    assert_eq!(s.read_i64_be().unwrap(), 7);
    assert_eq!(s.read_i64_le().unwrap(), i64::MIN);
    let mut f = stream(&[0x3F, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0xF0, 0x3F]);
    assert_eq!(f.read_f32_be().unwrap(), 1.0);
    assert_eq!(f.read_f64_le().unwrap(), 1.0);
}

#[test]
fn b1_fixed_size_read_at_end_is_syrp_io_0001() {
    let mut s = stream(&[1, 2, 3]);
    let err = s.read_i32_be().unwrap_err();
    assert_eq!(err.kind(), IoErrorKind::EndOfStream);
    assert_eq!(err.code().as_str(), "SYRP-IO-0001");
    assert_eq!(
        stream(&[]).read_u8().unwrap_err().kind(),
        IoErrorKind::EndOfStream
    );
    assert_eq!(
        stream(&[0; 7]).read_f64_le().unwrap_err().kind(),
        IoErrorKind::EndOfStream
    );
}

#[test]
fn b1_compressed_int_layout_and_bad_length() {
    let mut s = stream(&[0x00, 0x01, 0xFF, 0x84, 0, 0, 0, 0x80, 0x80]);
    assert_eq!(s.read_compressed_int().unwrap(), 0);
    assert_eq!(s.read_compressed_int().unwrap(), 255);
    assert_eq!(s.read_compressed_int().unwrap(), i32::MIN);
    assert_eq!(
        s.read_compressed_int().unwrap(),
        0,
        "negative zero reads as zero"
    );
    let err = stream(&[0x05, 1, 2, 3, 4, 5])
        .read_compressed_int()
        .unwrap_err();
    assert_eq!(err.kind(), IoErrorKind::InvalidData);
    assert_eq!(err.code().as_str(), "SYRP-IO-0004");
    assert_eq!(
        stream(&[0x04, 0xFF, 0xFF, 0xFF, 0x7F])
            .read_compressed_int()
            .unwrap(),
        i32::MAX
    );
    for overlong in [
        [0x04, 0xFF, 0xFF, 0xFF, 0xFF],
        [0x84, 0xFF, 0xFF, 0xFF, 0xFF],
        [0x84, 1, 0, 0, 0x80],
    ] {
        let err = stream(&overlong).read_compressed_int().unwrap_err();
        assert_eq!(err.code().as_str(), "SYRP-IO-0004", "{overlong:02X?}");
    }
}

#[test]
fn b1_strings_and_lines() {
    let mut s = stream(b"one\0two");
    assert_eq!(s.read_string().unwrap(), "one");
    assert_eq!(s.read_string().unwrap(), "two");
    assert_eq!(s.read_string().unwrap(), "");
    let mut l = stream(b"a\r\nb\n\nc\rd");
    assert_eq!(l.read_next_line().unwrap(), "a");
    assert_eq!(l.read_next_line().unwrap(), "b");
    assert_eq!(l.read_next_line().unwrap(), "");
    assert_eq!(l.read_next_line().unwrap(), "c\rd");
    assert!(l.is_exhausted());
}

#[test]
fn b1_entire_stream_as_string_honours_byte_order_marks() {
    let read = |b: &[u8]| stream(b).read_entire_stream_as_string().unwrap();
    assert_eq!(read("héllo".as_bytes()), "héllo");
    assert_eq!(read(b"\xEF\xBB\xBFbom"), "bom");
    assert_eq!(read(&[0xFF, 0xFE, 0x3D, 0xD8, 0x00, 0xDE]), "\u{1F600}");
    assert_eq!(read(&[0xFE, 0xFF, 0, b'h', 0, b'i']), "hi");
    assert_eq!(read(b"bad\xFF"), "bad\u{FFFD}");
    let mut s = stream(b"xyz");
    s.read_u8().unwrap();
    assert_eq!(s.read_entire_stream_as_string().unwrap(), "yz");
}

#[test]
fn b1_read_into_memory_block_appends_with_limit() {
    let mut s = stream(b"abcdef");
    let mut block = MemoryBlock::from_slice(b">");
    assert_eq!(s.read_into_memory_block(&mut block, Some(2)).unwrap(), 2);
    assert_eq!(s.read_into_memory_block(&mut block, None).unwrap(), 4);
    assert_eq!(s.read_into_memory_block(&mut block, None).unwrap(), 0);
    assert_eq!(&block[..], b">abcdef");
}

/// A non-seekable source exposed through `Box<dyn InputStream>`.
struct Unknown(Cursor<Vec<u8>>);

impl Read for Unknown {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl InputStream for Unknown {
    fn total_length(&mut self) -> Option<u64> {
        None
    }
    fn position(&self) -> u64 {
        self.0.position()
    }
    fn set_position(&mut self, _: u64) -> Result<(), syrp_io::IoError> {
        Err(syrp_io::IoError::new(IoErrorKind::Io))
    }
}

#[test]
fn b1_unknown_length_and_boxed_streams() {
    let mut s: Box<dyn InputStream> =
        Box::new(Unknown(Cursor::new(b"\x01\x00\x00\x00rest".to_vec())));
    assert_eq!(s.total_length(), None);
    assert!(!s.is_exhausted());
    assert_eq!(s.read_i32_le().unwrap(), 1);
    assert!(s.set_position(0).is_err());
    let mut block = MemoryBlock::new();
    assert_eq!(s.read_into_memory_block(&mut block, None).unwrap(), 4);
    assert!(!s.is_exhausted(), "unknown length never reports exhaustion");
}

#[test]
fn b1_adapter_starts_at_inner_position_and_gives_it_back() {
    let mut c = Cursor::new(b"abc".to_vec());
    c.set_position(2);
    let mut s = StdInputStream::new(c).unwrap();
    assert_eq!(s.position(), 2);
    assert_eq!(s.read_u8().unwrap(), b'c');
    assert_eq!(s.get_ref().position(), 3);
    assert_eq!(s.into_inner().position(), 3);
}
