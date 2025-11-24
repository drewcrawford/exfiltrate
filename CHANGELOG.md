# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Complete rewrite from the ground up** - We tore everything apart and put it back together better. The entire architecture has been reimagined to be more modular, more powerful, and friendlier to work with.

- **Split into a workspace of specialized crates** - What was once a single crate is now a family of four:
  - `exfiltrate` - The core library with commands, wire protocol, and logwise hook
  - `exfiltrate_cli` - A dedicated CLI binary for connecting to your apps
  - `exfiltrate_proxy` - WebSocket/TCP bridge for browser and WASM targets
  - `exfiltrate_internal` - Shared types that keep everyone on the same page

- **Brand new wire protocol** - MessagePack-based serialization that's faster and more reliable than before. Your debug sessions will thank you.

### Added

- **First-class WASM support** - Debug your browser-based Rust apps! The new proxy bridges WebSocket connections for WASM targets running in browsers. Shared memory and atomics support included (just make sure your runtime likes them too).

- **Comprehensive built-in help system** - Get help on custom commands, integration guides, and more without leaving your terminal.

- **Local commands** - `help`, `list`, and `status` commands available right in the CLI.

- **logwise integration** - Enable the `logwise` feature to collect logs from your running application. Keep it disabled by default to avoid shipping sensitive data.

- **Example debug server** - The `exfiltrate/examples/debug.rs` shows exactly how to embed the library in your own project.

- **SPDX license headers** - All source files now proudly display their license information.

### Fixed

- **Clippy is happy** - Resolved all clippy warnings to keep the codebase warning-free.

- **CI workflow syntax** - Fixed YAML indentation that was making the linter grumpy.

### Documentation

- Added comprehensive CLAUDE.md with project guidelines for AI-assisted development
- Improved inline documentation throughout the codebase
- Added README with usage examples and architecture overview

## [0.1.0] - Initial Release

The first release of exfiltrate - an embeddable debug tool for Rust.
