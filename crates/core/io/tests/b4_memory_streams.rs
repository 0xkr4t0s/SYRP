//! B4: `MemoryInputStream` and `MemoryOutputStream`.

use std::io::Write;

use proptest::prelude::*;
use syrp_error::Diagnostic;
use syrp_io::{
    InputStream, IoErrorKind, MemoryBlock, MemoryInputStream, MemoryOutputStream, OutputStream,
};

#[test]
fn b4_input_borrowed_and_owned_read_the_same() {
    let bytes = b"\x00\x2a\x01line\nrest";
    let streams = [
        MemoryInputStream::new(bytes),
        MemoryInputStream::from_vec(bytes.to_vec()),
        MemoryInputStream::from_block(MemoryBlock::from_slice(bytes)),
    ];
    for mut s in streams {
        assert_eq!(s.total_length(), Some(bytes.len() as u64));
        assert_eq!(s.read_i16_be().unwrap(), 42);
        assert!(s.read_bool().unwrap());
        assert_eq!(s.read_next_line().unwrap(), "line");
        assert_eq!(s.position(), 8);
        s.set_position(1).unwrap();
        assert_eq!(s.read_u8().unwrap(), 0x2a);
        assert_eq!(s.skip_next_bytes(100).unwrap(), 10);
        assert!(s.is_exhausted());
        assert_eq!(s.read_u8().unwrap_err().kind(), IoErrorKind::EndOfStream);
        assert_eq!(s.data(), bytes);
    }
}

#[test]
fn b4_input_set_position_past_end_clips_syrp_choice() {
    let mut s = MemoryInputStream::new(b"abc");
    s.set_position(10).unwrap();
    assert_eq!(s.position(), 3);
    assert!(s.is_exhausted());
    s.set_position(u64::MAX).unwrap();
    assert_eq!(s.position(), 3);
}

#[test]
fn b4_output_set_position_past_end_zero_fills_syrp_choice() {
    let mut s = MemoryOutputStream::new();
    s.write_text("ab", false, false, None).unwrap();
    s.set_position(5).unwrap();
    assert_eq!(s.len(), 5, "the gap is filled at once");
    assert_eq!(s.data(), b"ab\0\0\0");
    s.write_u8(b'c').unwrap();
    assert_eq!(s.data(), b"ab\0\0\0c");

    let mut buf = [9u8; 4];
    let mut f = MemoryOutputStream::over_slice(&mut buf);
    f.set_position(3).unwrap();
    assert_eq!(f.data(), &[0, 0, 0]);
    let err = f.set_position(5).unwrap_err();
    assert_eq!(err.kind(), IoErrorKind::BufferFull);
    assert_eq!(f.position(), 3, "a failed seek does not move");
}

#[test]
fn b4_output_overwrites_before_len_and_hands_over_its_block() {
    let mut s = MemoryOutputStream::new();
    s.write_i32_be(0x0102_0304).unwrap();
    s.set_position(1).unwrap();
    s.write_u8(9).unwrap();
    assert_eq!(s.len(), 4);
    assert_eq!(s.position(), 2);
    assert_eq!(s.data(), &[1, 9, 3, 4]);
    s.preallocate(1 << 16).unwrap();
    assert_eq!(s.data(), &[1, 9, 3, 4]);
    assert_eq!(&s.into_memory_block()[..], &[1, 9, 3, 4]);
}

#[test]
fn b4_output_appends_to_callers_block_and_reset_restores_it() {
    let mut block = MemoryBlock::from_slice(b">>");
    {
        let mut s = MemoryOutputStream::appending_to(&mut block);
        assert_eq!(s.position(), 0);
        s.write_string("hi").unwrap();
        assert_eq!(s.data(), b"hi\0");
        assert_eq!(s.to_utf8_string(), "hi\0");
        assert_eq!(&s.into_memory_block()[..], b"hi\0");
    }
    assert_eq!(&block[..], b">>hi\0");
    let mut s = MemoryOutputStream::appending_to(&mut block);
    s.write_u8(b'!').unwrap();
    s.reset();
    assert!(s.is_empty());
    assert_eq!(s.position(), 0);
    drop(s);
    assert_eq!(
        &block[..],
        b">>hi\0",
        "reset removes only this stream's bytes"
    );
}

#[test]
fn b4_fixed_slice_full_is_syrp_io_0002_and_writes_nothing() {
    let mut buf = [0u8; 5];
    let mut s = MemoryOutputStream::over_slice(&mut buf);
    s.write_i16_le(-1).unwrap();
    let err = s.write_i32_le(7).unwrap_err();
    assert_eq!(err.kind(), IoErrorKind::BufferFull);
    assert_eq!(err.code().as_str(), "SYRP-IO-0002");
    assert_eq!(s.len(), 2);
    assert_eq!(s.position(), 2);
    assert_eq!(
        s.preallocate(6).unwrap_err().kind(),
        IoErrorKind::BufferFull
    );
    s.write_text("abc", false, false, None).unwrap();
    s.reset();
    assert!(s.data().is_empty());
    drop(s);
    assert_eq!(buf, [0xFF, 0xFF, b'a', b'b', b'c']);
}

#[test]
fn b4_output_copies_from_an_input_stream() {
    let mut src = MemoryInputStream::new(b"0123456789");
    let mut s = MemoryOutputStream::new();
    assert_eq!(s.write_from_input_stream(&mut src, Some(4)).unwrap(), 4);
    assert_eq!(s.to_utf8_string(), "0123");
    let mut buf = [0u8; 3];
    let mut f = MemoryOutputStream::over_slice(&mut buf);
    let err = f.write_from_input_stream(&mut src, None).unwrap_err();
    assert_eq!(err.kind(), IoErrorKind::BufferFull);
    assert_eq!(err.bytes_copied(), Some(0));
}

#[derive(Debug, Clone)]
enum Op {
    Write(Vec<u8>),
    Seek(u8),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        proptest::collection::vec(any::<u8>(), 0..8).prop_map(Op::Write),
        any::<u8>().prop_map(|p| Op::Seek(p % 64)),
    ]
}

/// Applies `ops` to `s` and to a `Vec` model with Syrp's zero-fill rule.
fn run(s: &mut MemoryOutputStream<'_>, ops: &[Op]) -> Vec<u8> {
    let (mut model, mut pos) = (Vec::new(), 0usize);
    for op in ops {
        match op {
            Op::Write(bytes) => {
                s.write_all(bytes).unwrap();
                let end = pos + bytes.len();
                if model.len() < end {
                    model.resize(end, 0);
                }
                model[pos..end].copy_from_slice(bytes);
                pos = end;
            }
            Op::Seek(p) => {
                s.set_position(u64::from(*p)).unwrap();
                pos = usize::from(*p);
                if model.len() < pos {
                    model.resize(pos, 0);
                }
            }
        }
        assert_eq!(s.position(), pos as u64);
    }
    model
}

proptest! {
    #[test]
    fn b4_every_output_target_agrees_with_a_vec_model(ops in proptest::collection::vec(op(), 0..24)) {
        let mut owned = MemoryOutputStream::new();
        let model = run(&mut owned, &ops);
        prop_assert_eq!(owned.data(), &model[..]);

        let mut block = MemoryBlock::from_slice(b"prefix");
        let mut appending = MemoryOutputStream::appending_to(&mut block);
        prop_assert_eq!(&run(&mut appending, &ops), &model);
        prop_assert_eq!(appending.data(), &model[..]);
        drop(appending);
        prop_assert_eq!(&block[6..], &model[..]);

        let mut buf = [0xAAu8; 256];
        let mut fixed = MemoryOutputStream::over_slice(&mut buf);
        prop_assert_eq!(&run(&mut fixed, &ops), &model);
        prop_assert_eq!(fixed.data(), &model[..]);
    }
}
