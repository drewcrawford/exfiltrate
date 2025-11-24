# Repository Guidelines

## Project Structure & Module Organization
- Workspace crates: `exfiltrate` (core library: commands, wire protocol, logwise hook), `exfiltrate_cli` (CLI binary), `exfiltrate_proxy` (WebSocket/TCP bridge for browser/WASM targets), and `exfiltrate_internal` (shared types). Assets live in `art/`; the `exfiltrate/examples/debug.rs` sample demonstrates embedding.
- Rust target defaults to host; `.cargo/config.toml` configures `wasm32-unknown-unknown` with atomics/shared-memory flags and `wasm-server-runner` for browser-style testing.

## Build, Test, and Development Commands
- `cargo build --workspace` — Build all crates for the host; add `--release` for optimized binaries.
- `cargo run -p exfiltrate_cli -- --help` — Run the CLI; use `cargo run -p exfiltrate_cli -- connect <addr:port>` when pointing at a running server.
- `cargo run -p exfiltrate --example debug` — Launch the example host to validate CLI <-> library flows.
- `cargo run -p exfiltrate_proxy -- --help` — Start the proxy when debugging WASM/browser apps.
- `cargo test --workspace` — Execute unit/integration tests; add crates as needed (current suite is light).
- `cargo build -p exfiltrate --target wasm32-unknown-unknown` — Build the library for WASM; nightly is required for `build-std` in the provided config.
- `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER='wasm-bindgen-test-runner' cargo +nightly test --target=wasm32-unknown-unknown` is required to test WASM

## Coding Style & Naming Conventions
- Rust 2024 edition, 4-space indentation, module files in `snake_case.rs`, types/traits in `PascalCase`, functions in `snake_case`.
- Run `cargo fmt` before submitting; prefer `cargo clippy --workspace --all-targets -- -D warnings` to keep the codebase warning-free.
- Keep shared protocol/command logic in `exfiltrate_internal`; avoid duplicating wire definitions in the CLI or proxy.

## Testing Guidelines
- Co-locate unit tests in `#[cfg(test)] mod tests` blocks near the code they cover; use crate-level `tests/` for integration flows (CLI/proxy/library together).
- For WASM-sensitive code, avoid blocking primitives; rely on the existing atomics/shared-memory config and test via `wasm-server-runner` when feasible.
- When adding a new command or protocol change, include a round-trip test that exercises serialization via `rmp-serde`.

## Commit & Pull Request Guidelines
- Follow the existing history: short, imperative subjects (e.g., “Handle unknown variants”, “Split into crates”); capitalize the first word.
- PRs should describe behavior changes, mention any new feature flags (e.g., `logwise`), note target platforms touched (native vs WASM), and list the commands/tests you ran.
- Link issues when applicable; include screenshots or transcripts only when UI/CLI output materially changes.

## Security & Configuration Tips
- `logwise` is optional; enable it only when you intend to collect logs and avoid shipping sensitive data by default.
- The WASM build uses shared memory and atomics—verify your browser/runtime supports these flags before relying on them in demos.
