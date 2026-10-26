# Add an error code

**Goal:** a new `SYRP-<DOMAIN>-<NNNN>` code that your diagnostics carry and the catalogue explains.

**Preconditions**
- The failure is new: `cargo syrp docs search <words>` and `references/errors.json` show no
  existing code for it. Reuse an existing code when one fits.

**Steps**
1. Pick the domain file `errors/<domain>.toml` (any WP may add codes there). Take the next
   free number in that file. Codes are permanent: never renumber or reuse one.
2. Append an entry with exactly these string keys (plus an optional `example`):
   ```toml
   [[error]]
   code = "SYRP-<DOMAIN>-<NNNN>"
   title = "Short title"
   severity = "error"             # error | warning | info
   cause = "Why it happens, in your own words."
   fix = "What the reader does next."
   ```
3. Emit the code from your diagnostic (the crate's error type implements
   `syrp_error::Diagnostic` and returns the code).
4. Test the code path with a `b<n>_` test that asserts the code.
5. Refresh the skill's reference if your WP owns `skills/syrp/`: `scripts/refresh.sh`.

**New domain?** A new `errors/<domain>.toml` is fine, but `cargo syrp explain` only knows the
domains embedded in `crates/tooling/cli/src/catalogue.rs`. Today that is `cli`, `wp`, `pr` and
`conductor`; the `cleanroom`, `doc` and `error` files are not embedded. Do not edit the CLI if
you do not own it: open a `[CR]` change request asking for the domain to be added, and say so
in the PR body.

**Verification**
- `cargo syrp test --crate syrp-error --json` passes: its `b4_every_repository_catalogue_file_parses`
  test parses every `errors/*.toml` (one domain per file, five keys, valid severity, no duplicates).
- For an embedded domain, `cargo syrp explain <code>` prints your entry.

**Common errors**
- `SYRP-CLI-0002`: `explain` does not know the code; the domain is not embedded, or a typo.
- `SYRP-ERROR-0002`..`SYRP-ERROR-0005` (from `syrp-error`, not in `explain`): a missing or
  unknown key, a malformed code, an unknown severity, or a duplicate code.
