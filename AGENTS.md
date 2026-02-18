# Agents

## Schema tests

- Schema integration tests must assert full JSON schema equality using diff-based assertions (e.g. `similar_asserts::assert_eq!(actual, expected)`).
- Do not replace full-schema equality with selective assertions of a few fields.
- Avoid snapshot testing / auto-regeneration; if output changes intentionally, update the full expected schema fixtures explicitly.

## Result types

- Prefer explicit result types and avoid `Result` aliases (do not import `Result` as a local alias), to avoid confusion with `std::result::Result`.
- Inside crates, prefer typed error enums (e.g. `std::result::Result<T, MyError>`) for precise variants.
- Convert typed errors to `color_eyre::eyre::Report` only at the outer boundary (e.g. `main`).
