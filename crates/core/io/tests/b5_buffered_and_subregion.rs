//! B5: `BufferedInputStream` and `SubregionStream`.

use std::cell::Cell;
use std::io::Read;
use std::rc::Rc;

use proptest::prelude::*;
use syrp_io::{BufferedInputStream, InputStream, IoError, MemoryInputStream, SubregionStream};

/// A source that counts the bytes read from it and can hide its length.
struct Counting {
    inner: MemoryInputStream<'static>,
    bytes_read: Rc<Cell<u64>>,
    length_known: bool,
}

fn counting(data: &[u8], length_known: bool) -> (Counting, Rc<Cell<u64>>) {
    let bytes_read = Rc::new(Cell::new(0));
    let source = Counting {
        inner: MemoryInputStream::from_vec(data.to_vec()),
        bytes_read: Rc::clone(&bytes_read),
        length_known,
    };
    (source, bytes_read)
}

impl Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read.set(self.bytes_read.get() + n as u64);
        Ok(n)
    }
}

impl InputStream for Counting {
    fn total_length(&mut self) -> Option<u64> {
        self.inner.total_length().filter(|_| self.length_known)
    }
    fn position(&self) -> u64 {
        self.inner.position()
    }
    fn set_position(&mut self, position: u64) -> Result<(), IoError> {
        self.inner.set_position(position)
    }
}

const DATA: &[u8] = b"0123456789abcdefghij";

#[test]
fn b5_seeking_inside_the_buffer_does_not_reread_the_source() {
    let (source, bytes_read) = counting(DATA, true);
    let mut s = BufferedInputStream::new(source, 8).unwrap();
    assert_eq!(s.read_u8().unwrap(), b'0');
    assert_eq!(bytes_read.get(), 8, "one block read ahead");
    s.set_position(6).unwrap();
    assert_eq!(s.read_u8().unwrap(), b'6');
    s.set_position(0).unwrap();
    assert_eq!(s.read_i16_be().unwrap(), i16::from_be_bytes(*b"01"));
    s.set_position(8).unwrap();
    assert_eq!(
        bytes_read.get(),
        8,
        "no seek inside the block touched the source"
    );
    assert_eq!(s.read_u8().unwrap(), b'8');
    assert_eq!(bytes_read.get(), 16);
    s.set_position(2).unwrap();
    assert_eq!(
        s.read_u8().unwrap(),
        b'2',
        "outside the block: the source is re-read"
    );
    assert_eq!(bytes_read.get(), 24);
}

#[test]
fn b5_peek_byte_does_not_consume_and_is_none_at_the_end() {
    let (source, _) = counting(b"ab", false);
    let mut s = BufferedInputStream::new(source, 4).unwrap();
    assert_eq!(s.peek_byte().unwrap(), Some(b'a'));
    assert_eq!(s.peek_byte().unwrap(), Some(b'a'));
    assert_eq!(s.position(), 0);
    assert!(!s.is_exhausted());
    assert_eq!(s.read_entire_stream_as_string().unwrap(), "ab");
    assert_eq!(s.peek_byte().unwrap(), None);
    assert!(
        s.is_exhausted(),
        "end found by looking ahead, length unknown"
    );
    assert_eq!(s.total_length(), None);
}

#[test]
fn b5_large_reads_and_skips_bypass_the_buffer() {
    let (source, bytes_read) = counting(DATA, true);
    let mut s = BufferedInputStream::new(source, 4).unwrap();
    let mut big = [0u8; 10];
    s.read_exact(&mut big).unwrap();
    assert_eq!(&big, b"0123456789");
    assert_eq!(bytes_read.get(), 10);
    assert_eq!(s.skip_next_bytes(5).unwrap(), 5);
    assert_eq!(s.read_u8().unwrap(), b'f');
    assert_eq!(s.skip_next_bytes(100).unwrap(), 4);
    assert!(s.is_exhausted());
    assert_eq!(s.buffer_size(), 4);
}

#[test]
fn b5_both_own_a_boxed_source_and_give_it_back() {
    let boxed: Box<dyn InputStream> = Box::new(MemoryInputStream::new(DATA));
    let mut buffered = BufferedInputStream::new(boxed, 16).unwrap();
    assert_eq!(buffered.read_u8().unwrap(), b'0');
    let at = buffered.position();
    let mut inner = buffered.into_inner();
    inner.set_position(at).unwrap();
    let mut window = SubregionStream::new(inner, 10, Some(3)).unwrap();
    assert_eq!(window.read_entire_stream_as_string().unwrap(), "abc");
    assert_eq!(window.into_inner().position(), 13);
}

#[test]
fn b5_subregion_positions_are_relative_and_the_length_is_clipped() {
    let mut s = SubregionStream::new(MemoryInputStream::new(DATA), 15, Some(100)).unwrap();
    assert_eq!(s.start(), 15);
    assert_eq!(s.total_length(), Some(5));
    assert_eq!(s.position(), 0);
    assert_eq!(s.read_u8().unwrap(), b'f');
    s.set_position(4).unwrap();
    assert_eq!(s.read_u8().unwrap(), b'j');
    assert!(s.is_exhausted());
    s.set_position(50).unwrap();
    assert_eq!(s.position(), 5, "a seek past the window clips to its end");
    assert_eq!(s.get_ref().position(), 20);

    let mut past = SubregionStream::new(MemoryInputStream::new(DATA), 99, None).unwrap();
    assert_eq!(past.total_length(), Some(0));
    assert!(past.is_exhausted());
}

#[test]
fn b5_subregion_stops_at_its_end_even_when_the_source_has_no_length() {
    let (source, bytes_read) = counting(DATA, false);
    let mut s = SubregionStream::new(source, 2, Some(3)).unwrap();
    assert_eq!(s.total_length(), Some(3));
    assert_eq!(s.read_entire_stream_as_string().unwrap(), "234");
    assert_eq!(bytes_read.get(), 3, "nothing past the window is read");

    let (source, _) = counting(DATA, false);
    let mut open = SubregionStream::new(source, 18, None).unwrap();
    assert_eq!(open.total_length(), None);
    assert!(!open.is_exhausted());
    assert_eq!(open.read_entire_stream_as_string().unwrap(), "ij");
}

#[derive(Debug, Clone)]
enum Op {
    Read(u8),
    Seek(u8),
    Peek,
    Skip(u8),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<u8>().prop_map(|n| Op::Read(n % 24)),
        any::<u8>().prop_map(|p| Op::Seek(p % 24)),
        Just(Op::Peek),
        any::<u8>().prop_map(|n| Op::Skip(n % 24)),
    ]
}

proptest! {
    #[test]
    fn b5_buffered_and_subregion_agree_with_a_memory_stream(
        ops in proptest::collection::vec(op(), 0..32),
        buffer_size in 1usize..12,
        start in 0u64..6,
        length in 0u64..20,
    ) {
        let window = &DATA[start as usize..(start + length).min(DATA.len() as u64) as usize];
        let mut model = MemoryInputStream::new(window);
        let inner = SubregionStream::new(MemoryInputStream::new(DATA), start, Some(length)).unwrap();
        let mut s = BufferedInputStream::new(inner, buffer_size).unwrap();
        for op in &ops {
            match *op {
                Op::Read(n) => {
                    let (mut a, mut b) = (vec![0u8; n.into()], vec![0u8; n.into()]);
                    let got = s.read(&mut a).unwrap();
                    let mut want = 0;
                    while want < got {
                        want += model.read(&mut b[want..got]).unwrap();
                    }
                    prop_assert_eq!(&a[..got], &b[..got]);
                }
                Op::Seek(p) => {
                    s.set_position(p.into()).unwrap();
                    model.set_position(p.into()).unwrap();
                }
                Op::Peek => {
                    let at = model.position();
                    let next = model.read_u8().ok();
                    model.set_position(at).unwrap();
                    prop_assert_eq!(s.peek_byte().unwrap(), next);
                }
                Op::Skip(n) => {
                    prop_assert_eq!(s.skip_next_bytes(n.into()).unwrap(), model.skip_next_bytes(n.into()).unwrap());
                }
            }
            prop_assert_eq!(s.position(), model.position());
            prop_assert_eq!(s.is_exhausted(), model.is_exhausted());
        }
    }
}
