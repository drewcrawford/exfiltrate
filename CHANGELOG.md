# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2025-12-20

### Changed
- Upgraded to logwise v0.5.0 for enhanced logging capabilities—because who doesn't love clearer debug messages?
- Streamlined CI workflow configuration to make builds faster and less chatty

### Added
- Comprehensive check scripts for both native and WASM targets—now you can validate your changes with confidence
- Enhanced documentation across README files and inline docs—we're making sure everything's crystal clear
- Target-specific testing scripts that make cross-platform development feel like a breeze

### Internal
- Reorganized build scripts into `scripts/` directory with dedicated native and WASM paths
- Added dedicated check, clippy, docs, fmt, and test scripts for streamlined development

## [0.2.0] - 2025-12-20

Initial tracked release of exfiltrate—your friendly embeddable debug companion for Rust.

[unreleased]: https://github.com/drewcrawford/exfiltrate/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/drewcrawford/exfiltrate/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/drewcrawford/exfiltrate/releases/tag/v0.2.0
