//! B6: the `InputSource` trait.

use std::collections::HashMap;

use syrp_io::{InputSource, InputStream, IoError, IoErrorKind, MemoryInputStream};

/// A source over in-memory "files": its own data plus neighbours by relative path.
struct Files {
    main: Vec<u8>,
    neighbours: HashMap<&'static str, Vec<u8>>,
}

impl InputSource for Files {
    fn create_input_stream(&self) -> Result<Box<dyn InputStream + Send>, IoError> {
        Ok(Box::new(MemoryInputStream::from_vec(self.main.clone())))
    }

    fn create_input_stream_for(
        &self,
        relative_path: &str,
    ) -> Result<Box<dyn InputStream + Send>, IoError> {
        let bytes = self
            .neighbours
            .get(relative_path)
            .ok_or(IoError::new(IoErrorKind::Io))?;
        Ok(Box::new(MemoryInputStream::from_vec(bytes.clone())))
    }

    fn hash_code(&self) -> u64 {
        self.main.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
        })
    }
}

fn files(main: &[u8]) -> Files {
    Files {
        main: main.to_vec(),
        neighbours: HashMap::from([("side.txt", b"side".to_vec())]),
    }
}

#[test]
fn b6_each_stream_is_fresh_and_independent() {
    let source: Box<dyn InputSource> = Box::new(files(b"\x00\x07tail"));
    let mut a = source.create_input_stream().unwrap();
    assert_eq!(a.read_i16_be().unwrap(), 7);
    let mut b = source.create_input_stream().unwrap();
    assert_eq!(b.position(), 0);
    assert_eq!(b.total_length(), Some(6));
    assert_eq!(a.read_entire_stream_as_string().unwrap(), "tail");
    assert_eq!(b.read_i16_be().unwrap(), 7);
}

#[test]
fn b6_relative_paths_open_neighbours_or_fail() {
    let source = files(b"main");
    let mut side = source.create_input_stream_for("side.txt").unwrap();
    assert_eq!(side.read_entire_stream_as_string().unwrap(), "side");
    let Err(err) = source.create_input_stream_for("missing") else {
        panic!("a missing neighbour must fail");
    };
    assert_eq!(err.kind(), IoErrorKind::Io);
}

#[test]
fn b6_streams_are_send_and_hash_identifies_the_data() {
    let source = files(b"payload");
    let stream = source.create_input_stream().unwrap();
    let text = std::thread::spawn(move || {
        let mut stream = stream;
        stream.read_entire_stream_as_string().unwrap()
    })
    .join()
    .unwrap();
    assert_eq!(text, "payload");
    assert_eq!(source.hash_code(), files(b"payload").hash_code());
    assert_ne!(source.hash_code(), files(b"other").hash_code());
}
