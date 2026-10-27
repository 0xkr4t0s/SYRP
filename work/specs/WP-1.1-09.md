# WP-1.1-09: syrp-net: URL, WebInputStream, URLInputSource and downloads

## Purpose
The HTTP side of `syrp-net` from master plan §5.2 (L2 Core) and Wave 1.1 item 9. It adds to the crate WP-1.1-10 creates. `WebInputStream` is a `syrp-io` `InputStream` (WP-1.1-03), `URL` converts to and from `File` (WP-1.1-04), and downloads run on a `syrp-thread` thread (WP-1.1-06). Master plan §5.3 names `ureq` with `rustls` for HTTP. Delivery: 3 PRs (B1, then B2, then B3–B4).

## Behaviour
- B1. `Url`: a value parsed leniently, as the `URL` class documentation describes (it is not a strict RFC 3986 parser), with `scheme`, `domain`, `port` (the scheme's default when absent), `sub_path`, ordered query `parameter_names`/`parameter_values`, `anchor`, `to_string(include_get_parameters)`, `is_well_formed`, `child_url`, `parent_url`, `with_new_domain_and_path`, `with_new_sub_path`, `with_parameter`, `with_parameters`, `with_anchor`, `with_post_data` (text or bytes), `with_file_to_upload(name, file, mime)`, `with_data_to_upload`, `file_name`, `is_local_file`, `local_file`, `from_file`, and the free functions `add_escape_chars(text, is_parameter, round_brackets_legal)`, `remove_escape_chars`, `is_probably_a_website_url` and `is_probably_an_email_address`. Tests: a vector table written from the documentation; a proptest that escaping then unescaping is the identity; a proptest that parsing never panics. A strict URL crate may be used internally only if it reproduces this leniency.
- B2. `WebInputStream::new(url, use_post)` with builder options for extra headers, timeout, the number of redirects to follow, a custom request command (`PUT`, `DELETE`, ...) and a progress callback. `connect() -> Result`, `status_code`, `response_headers` (ordered, multi-valued), `request_headers`, `is_error` and `cancel()` (from any thread, unblocks a read). It implements `InputStream`: `total_length` comes from `Content-Length` when known; `set_position` moves forward by reading and skipping, and backward is a coded error. Multipart bodies are built for uploads (B1). HTTPS uses `rustls` with the platform verifier or `webpki-roots` (choose one and justify it in `deps_added`). Tests use a loopback HTTP server started in the test (`std::net::TcpListener` on a thread) for GET, POST, headers, redirects and their limit, chunked transfer, a 404 status, timeout and cancel; HTTPS is tested against a loopback `rustls` server with a test-only certificate (`rcgen` as a dev-dependency). CI never touches the internet.
- B3. Conveniences: `Url::create_input_stream(options) -> Result<Box<dyn InputStream + Send>>` (`file:` URLs give a `FileInputStream`, `http` and `https` a `WebInputStream`), `read_entire_text_stream`, `read_entire_binary_stream`, `download_to_file(target, DownloadOptions) -> DownloadTask` (a background thread with progress, `cancel`, `is_finished`, `status_code` and `had_error`; on error or cancel the partial file is deleted) and `launch_in_default_browser` (only its error path is tested on CI). `UrlInputSource` implements `InputSource`.
- B4. Errors continue `errors/net.toml`; an HTTP status other than 2xx is not an error by itself (it is reported through `status_code`), but a connection failure is.

## Platforms
Linux, macOS and Windows are required, and CI must pass B1–B4 on all three. The HTTP stack is pure Rust on every platform; platform HTTP stacks (`NSURLSession`, WinHTTP) are not used.

## Not in this WP
`readEntireXmlStream` (XML is not in `syrp-var` yet), cookies and authentication helpers beyond headers, HTTP/2, WebSockets.

## Crate setup
The whole crate `crates/core/net/`, which WP-1.1-10 creates; this WP starts after it merges and may edit any file there, including `Cargo.toml` and `AGENTS.md` (add this WP's ledger rows, invariants and gaps). No new crate.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-09` first and `cargo syrp wp check WP-1.1-09` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
