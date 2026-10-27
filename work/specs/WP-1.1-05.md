# WP-1.1-05: syrp-io: gzip and zlib streams, ZipFile reader and builder

## Purpose
Compression in `syrp-io` from master plan §5.2 (L2 Core) and Wave 1.1 item 5. `syrp-state` persistence and the JUCE-compatible state formats (Wave 1.2 item 8) use the gzip streams, and the bundler and resource tools use zip. Master plan §5.3 lists `flate2` and `zip` as the third-party crates for this. Delivery: 2 PRs (B1–B2, then B3–B5).

## Behaviour
- B1. `GzipCompressorOutputStream<W: OutputStream>`: a compression level of 0–9 or the default, and a format of zlib (RFC 1950), gzip (RFC 1952) or raw deflate (RFC 1951), with the default the class documentation gives. `flush()` writes a sync flush point; `finish()` writes the trailer and returns the inner stream. Dropping an unfinished stream finishes it and ignores the error; say so in the docs and prefer `finish()` in examples.
- B2. `GzipDecompressorInputStream<R: InputStream>`: the same three formats plus auto-detection of gzip or zlib from the header. An optional known uncompressed size becomes `total_length`, otherwise `None`. `set_position` moves forward by decoding and skipping; moving backward restarts from the source's start when the source can seek, and fails with a coded error otherwise. Corrupt or truncated input returns a coded error and never panics. Tests: proptest round trip for every format and level over random data, including empty input; fixtures made by `gzip -9` and by `zlib` in another language, committed under `tests/data/` with their SHA-256 in a comment; a proptest that flips random bytes of valid streams and asserts error-or-success without a panic.
- B3. `ZipFile` reading: open from a `File`, an `InputSource` or a seekable `InputStream`. `num_entries`, `entry(i) -> &ZipEntry` (name with `/` separators, uncompressed size, modification `Time`, is-symlink, external attributes), `index_of_file_name(name, ignore_case)`, `entry_by_name`, `create_stream_for_entry(i) -> Result<Box<dyn InputStream + '_>, IoError>` for the stored and deflate methods (any other method is a coded error), and `sort_entries_by_filename`. Zip64 archives read correctly.
- B4. Extraction: `uncompress_to(dir, OverwriteFiles, FollowSymlinks)` and `uncompress_entry(i, dir, ...)` return `Result` and never write outside `dir`. An entry name that is absolute or climbs out with `..` is rejected (zip-slip; tested with crafted archives). Symlink entries are created only on Unix and only when asked; otherwise they are extracted as regular files holding the link text, as documented.
- B5. `ZipBuilder` (JUCE's `ZipFile::Builder`): `add_file(file, level, stored_path)`, `add_entry(Box<dyn InputStream>, level, stored_path, Time)` and `write_to_stream(&mut dyn OutputStream, progress: Option<&mut dyn FnMut(f64)>)`. Proptest: a random tree of names and contents is built, read back with B3, and compared (bytes, names and times to the two-second DOS resolution). An archive built here also passes an independent reader (the `zip` crate as a dev-dependency, or `unzip -t` when present on the runner).

Errors continue `errors/io.toml`. Use `flate2` with its pure-Rust backend (no C zlib), and either `zip` with `default-features = false` and only its deflate feature (its defaults build bzip2, zstd and lzma C code) or an in-crate zip container; list each in `deps_added`.

## Not in this WP
Encrypted archives, compression methods other than stored and deflate, writing Zip64.

## Crate setup
The whole crate `crates/core/io/`, which WP-1.1-03 creates; this WP starts after it merges and may edit any file there, including `Cargo.toml` and `AGENTS.md` (add this WP's ledger rows, invariants and gaps). No new crate.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-05` first and `cargo syrp wp check WP-1.1-05` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
