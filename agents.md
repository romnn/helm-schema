# Agents

## Schema tests

- Schema integration tests must assert full JSON schema equality using diff-based assertions (e.g. `similar_asserts::assert_eq!(actual, expected)`).
- Do not replace full-schema equality with selective assertions of a few fields.
- Avoid snapshot testing / auto-regeneration; if output changes intentionally, update the full expected schema fixtures explicitly.
