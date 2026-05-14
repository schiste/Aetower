# Bincode v1 → v2 Migration Plan

## Status

Open. Tracked via the RUSTSEC-2025-0141 ignore in `deny.toml`.

## Why this exists

`bincode` v1 (1.3.x) is the format used for `history.db` snapshot blobs in
`aetower-persistence`. Upstream has shipped v2 with breaking wire-format
changes. Cargo-deny will flag bincode v1 indefinitely until either:

1. The dependency is upgraded to v2 with an on-disk migration, or
2. The serialization format is replaced (e.g. with `postcard` or `serde_json`
   for the snapshot blob).

A silent dependency bump would corrupt every user's `history.db`. This document
exists so the ignore in `deny.toml` is anchored to a real plan, not a comment.

## Where bincode is used

Four call sites — all in `rust/crates/aetower-persistence/src/lib.rs`:

| Site | Direction | Purpose |
| --- | --- | --- |
| `lib.rs:1381` | deserialize | Hot path: decode `PersistedSnapshotEnvelope` for `format_version >= SNAPSHOT_FORMAT_VERSION` (currently 2). |
| `lib.rs:1386` | deserialize | Legacy fallback: decode a bare `SystemSnapshot` for older `format_version` values. |
| `lib.rs:1394` | serialize | Write path: encode the envelope on `store_snapshot`. |
| `lib.rs:1583` | serialize | Test fixture: produces a v1-style legacy blob to verify the fallback path still works. |

`SNAPSHOT_FORMAT_VERSION` is incremented whenever the envelope or its contents
change in a breaking way. The fallback at site 2 exists because user databases
predate the envelope wrapper.

## Constraints

- `history.db` lives in `~/Library/Application Support/Aetower/` and can grow to
  hundreds of megabytes (see prior 551 MB observation). A migration that
  rewrites every row in place must be incremental — a single transaction
  rewriting the entire table is unacceptable on user hardware.
- Aetower ships outside the App Store. Users update by replacing the bundle, so
  a "downgrade" can roll back a database that has already been migrated. The
  new format must either be readable by the previous binary or the migration
  must be one-way with a tombstone.
- The collector pipeline runs every ~500 ms. Anything that pauses writes during
  the migration must not stall the live snapshot stream.

## Proposed migration

Bump `SNAPSHOT_FORMAT_VERSION` to `3` and introduce a new column or magic byte
that identifies the new encoding, so the loader can route between formats per
row rather than relying on a single global flag.

Two acceptable target encodings:

1. **`postcard`** — compact, schema-driven, serde-compatible. Smaller than v1
   bincode on typical snapshots. No on-disk header changes required beyond the
   format-version bump.
2. **`bincode` v2** — preserves the existing semantics but requires the new
   `encode`/`decode` derives and a configuration object. Less invasive in the
   long term; more invasive at the migration boundary.

Either way the migration runs as a background maintenance pass that rewrites a
bounded number of rows per tick until all `format_version < 3` rows are gone.
The existing `aetower-persistence` maintenance plumbing
(`maintain_with_policy`, `cancellable_maintenance`) already provides a model
for this.

## Acceptance criteria

- New writes use the chosen v2/postcard encoding under
  `format_version = 3`.
- Reads succeed for `format_version ∈ {1, 2, 3}` for at least one release cycle
  so users upgrading from older builds aren't stranded.
- Background maintenance rewrites legacy rows incrementally with a per-tick
  budget; no user-visible pause in snapshot ingestion.
- `deny.toml` removes the RUSTSEC-2025-0141 ignore (or moves it to a strict
  expiry).

## Out of scope

- Changing the schema of `SystemSnapshot` itself. This migration is only about
  the on-disk encoding, not the data model.
- Compressing the blob payload. Worth considering separately given the
  database-size constraint.
