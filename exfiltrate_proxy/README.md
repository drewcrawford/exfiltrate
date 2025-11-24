# exfiltrate_proxy

A WebSocket/TCP bridge that enables debugging of WebAssembly applications running in browsers using the `exfiltrate` crate.

## Overview

WebAssembly applications running in a browser cannot open raw TCP sockets—they are restricted to WebSockets. The `exfiltrate` CLI, however, communicates via TCP.

This proxy bridges that gap by running two servers:

1. **WebSocket Server (Port 1338)**: Accepts connections from the WASM application
2. **TCP Server (Port 1337)**: Accepts connections from the `exfiltrate` CLI

Messages are forwarded bidirectionally between these endpoints, allowing the CLI to debug browser-based applications as if they were local native processes.

## Usage

Start the proxy:

```bash
cargo run -p exfiltrate_proxy
```

Or if installed:

```bash
exfiltrate_proxy
```

The proxy will listen on:
- `127.0.0.1:1337` for TCP connections (CLI)
- `127.0.0.1:1338` for WebSocket connections (WASM app)

## Architecture

```
┌─────────────────┐     WebSocket      ┌─────────────────┐       TCP        ┌─────────────────┐
│   WASM App      │ ◄────────────────► │  exfiltrate     │ ◄──────────────► │  exfiltrate     │
│   (Browser)     │     Port 1338      │    _proxy       │    Port 1337     │     CLI         │
└─────────────────┘                    └─────────────────┘                  └─────────────────┘
```

### Connection Flow

1. Start the proxy
2. Your WASM application connects to `ws://127.0.0.1:1338`
3. The CLI connects to `127.0.0.1:1337`
4. Commands from the CLI are forwarded to the WASM app via WebSocket
5. Responses from the WASM app are forwarded back to the CLI via TCP

### Multiple CLI Clients

The TCP server supports multiple concurrent CLI connections. Messages received from the WebSocket (WASM app) are broadcast to all connected TCP clients.

## WASM Application Setup

Your WASM application should connect to the proxy's WebSocket endpoint instead of using the native TCP server. The `exfiltrate` library handles this automatically when compiled for `wasm32-unknown-unknown` targets.

## Debugging Tips

The proxy outputs connection events and message sizes to stderr:

```
Listening on 127.0.0.1:1337
New WebSocket connection established: 127.0.0.1:xxxxx
Distributor: Received message of size 42
Distributor: Broadcasting to 1 clients
```

## License

See [LICENSE](../LICENSE) for details.
