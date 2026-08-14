# OPCUASim

Cross-platform OPC UA simulation suite — desktop apps built with **Rust** · **Tauri 2** · **Vue 3 + TypeScript + Vite** and the [`async-opcua`](https://crates.io/crates/async-opcua) stack.

| Binary | Role |
|--------|------|
| **OPCUAMaster** | Master station / client — connect, browse, monitor, history, methods |
| **OPCUAServer** | Address-space simulator — folders, variables with simulation modes, optional writable nodes |

[中文文档](README_CN.md)

## Features

### OPCUAMaster — Client / Master Station

- **OPC UA DA** — connect to any OPC UA server, browse address space, read/write values
- **Security** — None / Sign / SignAndEncrypt; Anonymous, Username/Password, Certificate auth
- **Endpoint discovery** — query a server URL to enumerate available endpoints and their security profiles
- **Lazy-loading address browser** — infinite-depth tree, expand on demand
- **Smart variable collection** — pick an Object node to add all Variable descendants in one click
- **Subscription + Polling** — server push or client pull at a configurable interval, per-node `DataChangeFilter`
- **Real-time table** — searchable, multi-select with `Ctrl/Cmd+Click`, quality colour coding
- **Value & Write panel** — node attributes, manual read, value write back to writable nodes
- **History (HA)** — read raw history into a plot + table tab, quick ranges (1m … 24h)
- **Method calls** — auto-discover input/output arguments and invoke methods from the browser
- **Communication log** — bottom panel with direction filter, search, CSV export
- **Project files** — save/load all connections + groups as `.opcuaproj`
- **Certificate manager** — list, trust/reject, delete certificates in the local PKI

### OPCUAServer — Address-Space Simulator

- **Embedded OPC UA server** — defaults to `opc.tcp://0.0.0.0:4840`
- **Folder + Variable tree** — add folders and variables under `Objects`
- **Simulation modes** — `Static`, `Random`, `Sine`, `Linear` (Repeat/Bounce), `Script` (`evalexpr`)
- **Live values** — variable values update at their per-node interval and stream to the UI
- **Writable nodes** — toggle `RW` to let clients write through
- **Project files** — save/load the entire address space as `.opcuaproj`

## Development

### Prerequisites

- [Rust](https://rustup.rs/) 1.77+
- [Node.js](https://nodejs.org/) 18+
- [Tauri CLI](https://tauri.app/) — `cargo install tauri-cli`

### Build & Run

```bash
# install frontend dependencies (run from the repository root)
cd frontend && npm install
cd master-frontend && npm install

# run the Server simulator
cd crates/opcuaserver-app && cargo tauri dev

# run the Master station
cd crates/opcuamaster-app && cargo tauri dev
```

### Project Structure

```
OPCUASim/
├── crates/
│   ├── opcuasim-core/          # Core library: client, server, browse, subscription, polling, history, methods
│   ├── opcuaserver-app/        # OPCUAServer Tauri application
│   └── opcuamaster-app/        # OPCUAMaster Tauri application
├── frontend/                   # Server Vue 3 frontend
├── master-frontend/            # Master Vue 3 frontend
└── shared-frontend/            # Shared Vue components, i18n, styles
```

## Contributing

1. Fork and create a feature branch from `master`
2. `cargo fmt` and `cargo clippy --workspace -- -D warnings` before committing
3. Conventional commit prefixes: `feat:`, `fix:`, `refactor:`, `docs:`, `chore:`
4. Open a PR against `master`

## Changelog

See [CHANGELOG.md](CHANGELOG.md) and the [Releases](https://github.com/kelsoprotein-lab/OPCUASim/releases) page.

## macOS First Launch

The bundles are **not Apple-notarized** (no paid Developer Program). On first launch macOS shows *"OPCUAServer / OPCUAMaster cannot be opened — Apple could not verify…"* with only *Done* and *Move to Trash* buttons. This is the standard macOS 15 (Sequoia) block for ad-hoc-signed apps — the app is **not damaged**.

<details>
<summary><b>How to allow it (pick one)</b></summary>

**1. GUI path**

- Double-click the `.app`, see the block dialog, click *Done*.
- Open *System Settings → Privacy & Security*, scroll to the bottom.
- You'll see *"OPCUAServer was blocked…"* — click *Open Anyway* and enter your password.
- The next dialog has an *Open* button; click it. Subsequent launches go straight through.

**2. One-line Terminal**

```bash
xattr -dr com.apple.quarantine "/Applications/OPCUAServer.app"
xattr -dr com.apple.quarantine "/Applications/OPCUAMaster.app"
```

Strips the quarantine flag so macOS stops blocking.

</details>

## License

MIT
