# WP-1.1-08: syrp-process: ChildProcess, InterProcessLock, DynamicLibrary and Process

## Purpose
The process layer from master plan §5.2 (L2 Core) and Wave 1.1 item 8. The out-of-process plugin scanner, `syrp-ipc`'s child-process coordinator (Wave 1.2 item 2) and the plugin hosts build on it. It uses `syrp-io` (WP-1.1-04) for the lock-file location and paths, and `syrp-thread` (WP-1.1-06) to drain child output without deadlock. Delivery: 3 PRs (B1, then B2–B3, then B4–B5).

## Behaviour
- B1. `ChildProcess`: `start(args: &[impl AsRef<OsStr>], Streams { stdout, stderr })` and `start_command_line(&str, Streams)`, which splits on spaces and honours double quotes. Write the splitting rules into the rustdoc and test them with a vector table. Also `is_running`, `read_process_output(&mut [u8]) -> Result<usize>` (stdout and stderr interleaved when both are chosen), `read_all_process_output() -> Result<String>` (lossy UTF-8), `wait_for_process_to_finish(timeout) -> bool`, `exit_code() -> Option<u32>` and `kill()`. As the class documentation says, dropping a `ChildProcess` does not end the child. Tests spawn the test binary itself in a helper mode (an env var or argument), never `sh` or `cmd`, so they run the same on every platform: output capture, large output that would fill a pipe, exit codes, timeout and kill.
- B2. `InterProcessLock::new(name)`: `lock(timeout: Option<Duration>) -> Result<InterProcessLockGuard, ProcessError>`. It is re-entrant within one `InterProcessLock` (counted, as documented) and released when the last guard drops or the holding process dies. Per platform: on Unix, an advisory `flock` on a file in a per-user directory under the temp location, with the name sanitised to a legal file name (document the rule); on Windows, a named mutex in the `Local\` namespace, where an abandoned mutex counts as acquired. Tests: two child processes contend and one times out; a holder that is killed releases the lock.
- B3. `DynamicLibrary::open(path_or_name) -> Result<DynamicLibrary, ProcessError>`, `unsafe fn symbol<T>(name) -> Result<Symbol<'_, T>, ProcessError>` (the caller asserts the type) and `native_handle()`; dropping it closes the library. Per platform: `dlopen(RTLD_NOW | RTLD_LOCAL)` and `dlsym` on Unix, `LoadLibraryExW` and `GetProcAddress` on Windows; `libloading` may be used. Tests open the platform C runtime (`libc.so.6`, `libSystem.B.dylib`, `kernel32.dll`) and call a harmless function; a missing library or symbol gives a coded error that carries the OS message.
- B4. `Process`: `set_priority(ProcessPriority::{Low, Normal, High, Realtime})` (best effort, reports refusal), `is_foreground_process`, `make_foreground_process` (macOS and Windows; a documented no-op on Linux), `is_running_under_debugger`, `terminate()` (exits at once with a documented code) and `open_document(path_or_url, parameters) -> Result` (`xdg-open`, `open`, `ShellExecuteW`). CI tests only the error path of `open_document`, and never launches a real application.
- B5. Errors use the new `errors/process.toml` domain (`SYRP-PROCESS-0001`...). Every `unsafe` block has a `// SAFETY:` comment.

## Platforms
Linux, macOS and Windows are required, and CI must pass B1–B5 on all three. Other targets compile, and their functions return a documented "unsupported" error.

## Not in this WP
`ChildProcessCoordinator`, `ChildProcessWorker` and `ChildProcessManager` (`syrp-ipc`, Wave 1.2), `raisePrivilege`/`lowerPrivilege`, dock icon visibility (needs `syrp-app`).

## Crate setup
Create `crates/core/process/` with `Cargo.toml` (inherit `version`, `edition`, `rust-version`, `repository`, `authors`, `publish` from the workspace; `[lints] workspace = true`), `[package.metadata.syrp] layer = "L2"`, an `AGENTS.md` following `docs/agents/crate-agents-template.md`, and `src/lib.rs`. Workspace members are globbed, so no root edit is needed.
Licence: `license = "AGPL-3.0-only OR BUSL-1.1"`.

## Rules that apply to every WP
- Clean room: work only from this spec, the master plan (`docs/design/master-plan.md`) and public JUCE *documentation* for behaviour. Never fetch or recall JUCE source code.
- Name every test after the behaviour it proves, prefixed with its id, for example `b3_try_push_fails_when_full`.
- Run `cargo syrp wp show WP-1.1-08` first and `cargo syrp wp check WP-1.1-08` last. A PR is ready only when that command exits 0.
- Shared files (workspace `Cargo.toml`, CI, `deny.toml`) are not yours. If you need a new third-party dependency in `[workspace.dependencies]`, declare it with an explicit version in your crate's own `Cargo.toml` and list it in the PR front matter `deps_added`; the adversarial verifier reviews it.
