# exfiltrate_cli

Command-line client for the Exfiltrate debugging framework. The CLI connects to an application that has called `exfiltrate::begin()` (or to the `exfiltrate_proxy` bridge for browser/WASM builds) and forwards commands/responses over TCP.

## Build and run
- From the workspace root: `cargo run -p exfiltrate_cli -- --help` to see usage.
- Build a binary: `cargo build -p exfiltrate_cli` or install locally with `cargo install --path . --bin exfiltrate_cli`.
- Quick demo: in one terminal run `cargo run -p exfiltrate --example debug`; in another, run `cargo run -p exfiltrate_cli -- list` to see the sample commands exposed by the example server.

The binary is named `exfiltrate_cli` in this workspace; the help text uses `exfiltrate` as the invocation name, so feel free to alias/symlink if you prefer the shorter command.

## Default connection
The CLI opens a TCP connection to `127.0.0.1:1337` (see `exfiltrate_internal::wire::ADDR`). If the target app is down or unreachable, local commands still work but remote ones will report the connection error. Use `status` to check whether the client could connect.

## Commands built into the CLI
- `list` — shows local commands and any remote commands reported by the connected application (remote entries are hidden if a local command of the same name exists).
- `status` — reports whether the CLI could open the TCP connection to the target.
- `help <command|topic>` — prints detailed help. Topics include `integration` and `custom_commands`; if a command is not local, the CLI asks the remote application for its help text.

Remote commands are defined by the instrumented application and show up in `list` once the CLI is connected.

## Response handling
- `Response::String` is printed to stdout.
- `Response::Files` writes each attachment to the current directory using a random filename and the provided extension; optional remarks are printed before the paths.
- `Response::Images` writes lossless WebP files (random filenames) to the current directory; large transfers emit progress to stderr while downloading/encoding.

## WASM and browser targets
WebAssembly builds cannot accept the direct TCP connection. Run the proxy (`cargo run -p exfiltrate_proxy --`) to bridge WebSocket clients on port 1338 to the CLI’s TCP port 1337. Start the proxy before attaching the CLI to a browser-hosted application.
