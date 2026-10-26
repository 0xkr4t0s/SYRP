# Licensing

> **DRAFT.** These terms implement the project's proposed licence (open decision 1 in
> [docs/design/0002-open-decisions.md](docs/design/0002-open-decisions.md)). They have not
> been reviewed by counsel. Do not rely on them for a commercial decision yet.

Syrp uses two licence tiers.

## Interface crates: `MIT OR Apache-2.0`

The foundation (L0) and contract (L1) crates are permissively licensed so that third-party
libraries can target Syrp's interfaces freely:
`syrp-error`, `syrp-rt`, `syrp-geom`, `syrp-simd`, every `syrp-*-api` crate and `syrp-midi`.
Each of these declares `license = "MIT OR Apache-2.0"` in its `Cargo.toml`.
Texts: [LICENSES/MIT.txt](LICENSES/MIT.txt), [LICENSES/Apache-2.0.txt](LICENSES/Apache-2.0.txt).

## Everything else: `AGPL-3.0-only OR BUSL-1.1`

You may choose either licence.

- **BUSL-1.1** ([LICENSES/BUSL-1.1.txt](LICENSES/BUSL-1.1.txt)): free production use,
  including closed source, for individuals and organisations below USD 100,000 a year of
  revenue plus funding, plus education and small non-profits. Each version converts to
  `Apache-2.0 OR MIT` three years after its release.
- **AGPL-3.0-only** ([LICENSES/AGPL-3.0-only.txt](LICENSES/AGPL-3.0-only.txt)): free for
  anyone whose whole program is released under the AGPL.
- **Commercial licence**: organisations above the threshold that do not want the AGPL buy
  one. Pricing is not set yet.

`cargo syrp license attest` (planned) writes a local self-attestation file. No
registration or telemetry is ever required.

## Contributions

Code written by the project's own agents is owned by the steward and carries provenance
trailers (see [AGENTS.md](AGENTS.md)). External contributions will require a CLA; until that
is in place, external pull requests are not accepted.
