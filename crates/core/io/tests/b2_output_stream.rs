//! B2: the `OutputStream` trait, `StdOutputStream`, and round trips through B1 readers.

use std::io::{Cursor, Read, Write};

use proptest::prelude::*;
use syrp_io::{InputStream, IoError, IoErrorKind, OutputStream, StdInputStream, StdOutputStream};

type Out = StdOutputStream<Cursor<Vec<u8>>>;

fn out() -> Out {
    StdOutputStream::new(Cursor::new(Vec::new())).unwrap()
}

fn reader(s: Out) -> StdInputStream<Cursor<Vec<u8>>> {
    StdInputStream::new(Cursor::new(s.into_inner().into_inner())).unwrap()
}

fn bytes(s: Out) -> Vec<u8> {
    s.into_inner().into_inner()
}

#[test]
fn b2_compressed_int_layout() {
    let cases: [(i32, &[u8]); 7] = [
        (0, &[0x00]),
        (1, &[0x01, 0x01]),
        (255, &[0x01, 0xFF]),
        (256, &[0x02, 0x00, 0x01]),
        (-1, &[0x81, 0x01]),
        (i32::MAX, &[0x04, 0xFF, 0xFF, 0xFF, 0x7F]),
        (i32::MIN, &[0x84, 0x00, 0x00, 0x00, 0x80]),
    ];
    for (value, expected) in cases {
        let mut s = out();
        s.write_compressed_int(value).unwrap();
        assert_eq!(bytes(s), expected, "value {value}");
    }
}

#[test]
fn b2_strings_text_and_repeated_bytes() {
    let mut s = out();
    s.write_string("hé").unwrap();
    s.write_text("a\nb\r\nc", false, true, Some("\n")).unwrap();
    s.write_text("x\ny", false, false, None).unwrap();
    s.write_repeated_byte(b'-', 3).unwrap();
    s.write_text("é\n", true, true, Some("\r\n")).unwrap();
    s.write_text("z", true, false, None).unwrap();
    let mut expected = "hé\0a\nb\nc".as_bytes().to_vec();
    expected.extend_from_slice(b"x\ny---\xFF\xFE\xE9\x00\r\x00\n\x00z\x00");
    assert_eq!(bytes(s), expected);
}

#[test]
fn b2_position_and_new_line() {
    let mut s = out();
    assert_eq!(s.new_line(), syrp_io::DEFAULT_NEW_LINE);
    s.write_new_line().unwrap();
    s.set_new_line("\n");
    s.write_new_line().unwrap();
    assert_eq!(s.position(), 3);
    s.set_position(0).unwrap();
    s.write_u8(b'!').unwrap();
    assert_eq!(s.position(), 1);
    assert_eq!(bytes(s), b"!\n\n");
}

#[test]
fn b2_write_from_input_stream_copies_and_limits() {
    let mut src = StdInputStream::new(Cursor::new(vec![7u8; 20_000])).unwrap();
    let mut s = out();
    assert_eq!(
        s.write_from_input_stream(&mut src, Some(9000)).unwrap(),
        9000
    );
    assert_eq!(s.write_from_input_stream(&mut src, None).unwrap(), 11_000);
    assert_eq!(s.write_from_input_stream(&mut src, None).unwrap(), 0);
    assert_eq!(bytes(s), vec![7u8; 20_000]);
}

/// A source and sink that moves `left` bytes, ten at a time, and then fails.
struct Flaky {
    left: usize,
}

impl Read for Flaky {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.write(buf)
    }
}

impl Write for Flaky {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = buf.len().min(self.left).min(10);
        if n == 0 {
            return Err(std::io::Error::other("pulled the plug"));
        }
        self.left -= n;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl InputStream for Flaky {
    fn total_length(&mut self) -> Option<u64> {
        None
    }
    fn position(&self) -> u64 {
        0
    }
    fn set_position(&mut self, _: u64) -> Result<(), IoError> {
        Ok(())
    }
}

impl OutputStream for Flaky {
    fn position(&self) -> u64 {
        0
    }
    fn set_position(&mut self, _: u64) -> Result<(), IoError> {
        Ok(())
    }
    fn new_line(&self) -> &str {
        "\n"
    }
    fn set_new_line(&mut self, _: &str) {}
}

#[test]
fn b2_copy_failures_report_bytes_copied() {
    let err = out()
        .write_from_input_stream(&mut Flaky { left: 25 }, None)
        .unwrap_err();
    assert_eq!(err.kind(), IoErrorKind::Io);
    assert_eq!(err.bytes_copied(), Some(25));
    assert!(err.to_string().contains("25 bytes"), "{err}");
    assert!(std::error::Error::source(&err).is_some());

    let mut src = StdInputStream::new(Cursor::new(vec![0u8; 100])).unwrap();
    let err = Flaky { left: 5 }
        .write_from_input_stream(&mut src, None)
        .unwrap_err();
    assert_eq!(err.bytes_copied(), Some(0));
    assert!(err.to_string().contains("0 bytes"), "{err}");
}

/// One value of every typed writer.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    U8(u8),
    Bool(bool),
    I16(i16, bool),
    I32(i32, bool),
    I64(i64, bool),
    F32(u32, bool),
    F64(u64, bool),
    Compressed(i32),
    Str(String),
}

fn value() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<u8>().prop_map(Value::U8),
        any::<bool>().prop_map(Value::Bool),
        any::<(i16, bool)>().prop_map(|(v, le)| Value::I16(v, le)),
        any::<(i32, bool)>().prop_map(|(v, le)| Value::I32(v, le)),
        any::<(i64, bool)>().prop_map(|(v, le)| Value::I64(v, le)),
        any::<(u32, bool)>().prop_map(|(v, le)| Value::F32(v, le)),
        any::<(u64, bool)>().prop_map(|(v, le)| Value::F64(v, le)),
        any::<i32>().prop_map(Value::Compressed),
        "[^\u{0}]{0,12}".prop_map(Value::Str),
    ]
}

fn write(s: &mut dyn OutputStream, v: &Value) -> Result<(), IoError> {
    match v {
        Value::U8(x) => s.write_u8(*x),
        Value::Bool(x) => s.write_bool(*x),
        Value::I16(x, true) => s.write_i16_le(*x),
        Value::I16(x, false) => s.write_i16_be(*x),
        Value::I32(x, true) => s.write_i32_le(*x),
        Value::I32(x, false) => s.write_i32_be(*x),
        Value::I64(x, true) => s.write_i64_le(*x),
        Value::I64(x, false) => s.write_i64_be(*x),
        Value::F32(x, true) => s.write_f32_le(f32::from_bits(*x)),
        Value::F32(x, false) => s.write_f32_be(f32::from_bits(*x)),
        Value::F64(x, true) => s.write_f64_le(f64::from_bits(*x)),
        Value::F64(x, false) => s.write_f64_be(f64::from_bits(*x)),
        Value::Compressed(x) => s.write_compressed_int(*x),
        Value::Str(x) => s.write_string(x),
    }
}

fn read_back(s: &mut dyn InputStream, v: &Value) -> Result<Value, IoError> {
    Ok(match *v {
        Value::U8(_) => Value::U8(s.read_u8()?),
        Value::Bool(_) => Value::Bool(s.read_bool()?),
        Value::I16(_, true) => Value::I16(s.read_i16_le()?, true),
        Value::I16(_, false) => Value::I16(s.read_i16_be()?, false),
        Value::I32(_, true) => Value::I32(s.read_i32_le()?, true),
        Value::I32(_, false) => Value::I32(s.read_i32_be()?, false),
        Value::I64(_, true) => Value::I64(s.read_i64_le()?, true),
        Value::I64(_, false) => Value::I64(s.read_i64_be()?, false),
        Value::F32(_, true) => Value::F32(s.read_f32_le()?.to_bits(), true),
        Value::F32(_, false) => Value::F32(s.read_f32_be()?.to_bits(), false),
        Value::F64(_, true) => Value::F64(s.read_f64_le()?.to_bits(), true),
        Value::F64(_, false) => Value::F64(s.read_f64_be()?.to_bits(), false),
        Value::Compressed(_) => Value::Compressed(s.read_compressed_int()?),
        Value::Str(_) => Value::Str(s.read_string()?),
    })
}

proptest! {
    #[test]
    fn b2_every_typed_writer_round_trips_through_its_reader(values in proptest::collection::vec(value(), 0..32)) {
        let mut s = out();
        for v in &values {
            write(&mut s, v).unwrap();
        }
        let mut r = reader(s);
        for v in &values {
            prop_assert_eq!(&read_back(&mut r, v).unwrap(), v);
        }
        prop_assert!(r.is_exhausted());
    }
}
