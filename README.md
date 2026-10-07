# lotus

Magic: The Gathering domain code shared by
[ManaVault](https://github.com/cfbender/manavault) (collection and deck manager)
and [the-gathering](https://github.com/cfbender/the-gathering) (Commander game
tracker). It is a plain library crate with no app-specific schema, so it is
usable by any Rust project that works with Scryfall data or imports decklists.

## What is in it

| Module | Contents |
| --- | --- |
| `card` | `Finish`, `Condition`, `Color`, `Rarity`, `Legality`, `Game`, `Zone`, type-line helpers such as `is_basic_land` (snow basics count). |
| `ids` | `ScryfallId` and `OracleId` newtypes that only hold well-formed lowercase UUIDs. |
| `quantity` | `Quantity`, a positive count. |
| `name` | `normalize_name` and `match_key` for diacritic- and punctuation-insensitive card lookups. |
| `commander` | `can_be_commander` and `commander_pairing` (Partner, Partner with, Background, Doctor's companion, Friends forever). |
| `scryfall` | The Scryfall card model, bulk-data manifest parsing, streaming JSON-lines reader for bulk files, rulings, and the catalog import policy both apps apply. Feature `http` adds `ScryfallClient`. |
| `decklist` | Pasted link parsing for Moxfield, Archidekt, and ManaVault share links; typed models of each site's JSON mapped to a common `Decklist`; a ManaVault GraphQL pager with page, entry, byte, and time budgets; an SSRF allowlist for user-supplied hosts. Feature `http` adds `DecklistClient`. |

## Features

- `sqlx`: `sqlx::Type` implementations (sqlx 0.9, SQLite) so the newtypes and
  enums map to text columns directly. Every enum stores the same strings the
  Elixir apps use (`nonfoil`, `near_mint`, `commander`, `manavault`, ...).
- `http`: `reqwest` + `tokio` clients for Scryfall and the decklist sources.

Both are off by default.

## Using it

```toml
[dependencies]
lotus = { git = "https://github.com/cfbender/lotus", features = ["sqlx", "http"] }
```

```rust
use lotus::decklist::{DeckLink, DecklistClient};

let link = DeckLink::parse("https://moxfield.com/decks/abc123xyz")?;
let client = DecklistClient::builder("my-app/1.0 (contact@example.com)").build()?;
let deck = client.fetch(&link).await?;
for entry in deck.playable() {
    println!("{} x{} ({:?})", entry.name, entry.quantity, entry.zone);
}
```

## Development

The toolchain is pinned in `mise.toml`.

```sh
mise install
mise run check   # fmt --check, clippy -D warnings (with and without features), tests, docs
mise run test    # cargo test --all-features
```

## Conventions

- `unsafe` is forbidden. `unwrap`, `expect`, `panic!`, `todo!`, indexing, and
  `as` casts are denied outside tests. Clippy pedantic is on.
- Invalid states are unrepresentable: ids are validated on construction,
  quantities are positive, fixed-vocabulary columns are enums, and every
  decklist entry has a zone.
- Behavior is ported from the Elixir apps and their tests. Where the two apps
  disagreed or an Elixir implementation had a bug, the crate documents the
  choice at the item that makes it instead of silently copying one side.
- Network code never follows redirects, caps response bodies without trusting
  `Content-Length`, and resolves user-supplied hosts once and pins the
  connection to the checked address.

## License

MPL-2.0. See `LICENSE`.
