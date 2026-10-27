// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! B6: the parity scenarios under `parity/scenarios/formats/aiff/` hold exactly the B5 files
//! (read) and the B4 streams (write). Inputs only: expected outputs come from harness runs.
//! Run with `SYRP_BLESS=1` to regenerate them after changing the catalogue.

mod common;

use std::fmt::Write as _;
use std::path::PathBuf;

use common::files::catalogue;

const HEAD: &str = r#"  "juce_tag": "9.0.4",
  "tolerance": "exact",
  "wp": "WP-2.1-17",
  "ledger_rows": ["juce_audio_formats/AiffAudioFormat"],"#;

const RECORD: &str = r#"  "record": ["opened", "sample_rate", "num_channels", "length_in_samples", "bits_per_sample", "uses_floating_point_data", "samples_f32", "metadata_values"],"#;

fn read_scenario(name: &str, title: &str, bytes: &[u8]) -> String {
    let hex = bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    });
    format!(
        r#"{{
  "id": "formats/aiff/read/{name}",
  "title": "Read: {title}",
{HEAD}
  "docs": [{{"page": "AiffAudioFormat", "member": "createReaderFor", "says": "Makes a reader for a stream that holds audio in this format, or fails."}}],
  "op": "read",
  "input_hex": "{hex}",
{RECORD}
  "notes": "Built by syrp-format-aiff tests/common/files.rs from the public AIFF 1.3 and AIFF-C layouts. Record whether a reader is created and, if so, everything it reports."
}}
"#
    )
}

fn write_scenario(name: &str, bits: u32, float: bool) -> String {
    let full = 2f64.powi(i32::try_from(bits).unwrap() - 1);
    // Channel 0 walks the integer grid, channel 1 holds values off the grid and out of range.
    let left: Vec<String> = [-full, -1.0, 0.0, 1.0, full / 2.0, full - 1.0]
        .iter()
        .map(|k| format!("{:?}", k / full))
        .collect();
    let right = "0.3, -0.7, 1.5, -1.5, 0.0001, -1.0";
    let kind = if float { "float" } else { "integer" };
    format!(
        r#"{{
  "id": "formats/aiff/write/{name}",
  "title": "Write {bits}-bit {kind} AIFF with markers, loops and text, then read it back",
{HEAD}
  "docs": [{{"page": "AiffAudioFormat", "member": "createWriterFor", "says": "Makes a writer that stores audio in this format on a stream, or fails."}}],
  "op": "write",
  "write": {{
    "sample_rate": 44100.0,
    "bits_per_sample": {bits},
    "floating_point": {float},
    "channels": [[{}], [{right}]],
    "cues": [{{"id": 1, "position": 0, "label": "start"}}, {{"id": 2, "position": 4, "label": "end"}}],
    "loops": [{{"kind": "forward", "start": 0, "end": 4}}, {{"kind": "ping_pong", "start": 1, "end": 3}}],
    "text": {{"title": "Take 3", "artist": "Someone", "copyright": "2026 Someone", "comment": "first"}}
  }},
{RECORD}
  "notes": "Pass the cues, loops and text through the metadata JUCE's AIFF writer accepts for them, write, then record the file as re-read by AiffAudioFormat. The second channel tests rounding and clipping."
}}
"#,
        left.join(", ")
    )
}

#[test]
fn b6_scenarios_hold_the_test_files() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../parity/scenarios/formats/aiff");
    let mut want: Vec<(PathBuf, String)> = catalogue()
        .iter()
        .map(|c| {
            (
                dir.join(format!("read/{}.json", c.name)),
                read_scenario(c.name, c.title, &c.bytes),
            )
        })
        .collect();
    for (name, bits, float) in [
        ("int8", 8, false),
        ("int16", 16, false),
        ("int24", 24, false),
        ("int32", 32, false),
        ("float32", 32, true),
    ] {
        want.push((
            dir.join(format!("write/{name}.json")),
            write_scenario(name, bits, float),
        ));
    }
    let bless = std::env::var_os("SYRP_BLESS").is_some();
    for (path, text) in want {
        if bless {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &text).unwrap();
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        // Windows checkouts may turn line ends into CRLF.
        let on_disk: Vec<&str> = on_disk.lines().collect();
        assert_eq!(
            on_disk,
            text.lines().collect::<Vec<_>>(),
            "{} is stale: rerun with SYRP_BLESS=1",
            path.display()
        );
    }
}
