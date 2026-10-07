# lotus

Standalone Rust library of Magic: The Gathering domain code, consumed as a git
dependency by cfbender/manavault and cfbender/the-gathering. Keep it free of
anything specific to either app: no app schemas, no app queries, no app
configuration. Database support stays behind the `sqlx` feature and network
support behind the `http` feature.

## Commands

```sh
mise install        # pinned Rust toolchain
mise run check      # what CI runs; must be green before committing
mise run test       # cargo test --all-features
```

`mise run check` runs `cargo fmt --check`, clippy with `-D warnings` both with
no features and with `--all-features`, the tests both ways, and `cargo doc`.
It uses `--locked`, so commit `Cargo.lock` with dependency changes.

## Rules

- Lints are the design: `unsafe` is forbidden; `unwrap`, `expect`, `panic!`,
  `todo!`, `unimplemented!`, `dbg!`, indexing/slicing, and `as` casts are
  denied outside tests. Do not `allow` them to get code compiling; restructure
  with types, `Option`, `Result`, `get`, `TryFrom`, or checked arithmetic.
- Make invalid states unrepresentable. Validate at construction (`ScryfallId`,
  `Quantity`), use enums for fixed vocabularies, and keep `as_str` / `parse` /
  serde / sqlx encodings of an enum identical (a test in `tests/sqlx_types.rs`
  enforces this; extend it for new enums).
- Every public item has a doc comment (`missing_docs` warns, CI denies
  warnings).
- Port behavior faithfully from the Elixir apps and their tests. When the two
  apps disagree, or the Elixir code has a bug, pick the correct behavior,
  document the decision in a doc comment on the item, and tell the user
  instead of silently copying either side.
- Network code must not follow redirects, must cap bodies while streaming, and
  must only connect to user-supplied hosts after `Allowlist` has checked every
  resolved address.
- Tests should catch plausible wrong implementations: pick inputs where the
  wrong answer differs from the right one, and derive expected values from
  the Elixir tests or site documentation, not from the implementation.
- Commit messages use Conventional Commits. No co-author trailers.
