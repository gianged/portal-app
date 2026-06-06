# Portal e2e

A single headless-browser smoke test (`login → raise ticket → see it in the
list`) driven by [fantoccini](https://docs.rs/fantoccini) (a Rust WebDriver
client) against a fully running stack.

This is a **standalone crate** (it declares its own empty `[workspace]`), so it
is never built by the main `cargo build`/`cargo test --workspace` and never
blocks PR CI. The test is `#[ignore]` by default. Run it explicitly:

```sh
bash scripts/e2e.sh
```

`scripts/e2e.sh` brings up the dependency stack, the `server` + `workers`
binaries, the Trunk-served frontend on `:8081`, and `geckodriver` on `:4444`,
then runs `cargo test -p portal-e2e -- --ignored`.

## Prerequisites

- Firefox + `geckodriver` on `PATH`
- `trunk` (frontend dev server)
- Docker (dependency stack via `cargo make bootstrap`)
- Seed data applied (`infra/postgres/*seed.sql`). The test logs in as a seeded
  account — adjust `EMAIL` / `PASSWORD` in `tests/smoke.rs` to match your seed.

Selectors in `tests/smoke.rs` are intentionally resilient (by input type /
visible text) but may need tightening as the UI evolves.
