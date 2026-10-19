# Development

## Requirements

- Rust 1.99.1, installed through `rustup` (version pinned in `rust-toolchain.toml`), with `rustfmt` and `clippy`.
- A C toolchain and `pkg-config`.
- Native dependencies for the selected display backends. On Debian 13:

  ```sh
  sudo apt install build-essential pkg-config libwayland-dev libxkbcommon-dev \
    libx11-dev libxcursor-dev libxi-dev libxrandr-dev libxkbcommon-x11-0
  ```

## Common commands

Run the application:

```sh
cargo run --locked -p desktop --bin chess-coach
```

Run the workspace checks:

```sh
cargo fmt --all -- --check
cargo build --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The chess rules and ordinary workspace tests do not need Stockfish. The `engine` crate launches a local UCI executable only when analysis is requested. Set `STOCKFISH_EXECUTABLE` to select a binary explicitly; otherwise discovery checks `PATH`, then `/usr/games/stockfish`. The real-engine test runs when `STOCKFISH_EXECUTABLE` is set and reports a skip otherwise. Portable fake UCI processes exercise handshake, cancellation, replacement, flooding, and cleanup on every platform:

```sh
STOCKFISH_EXECUTABLE=/usr/games/stockfish cargo test -p engine --locked -- --nocapture
```

Install `cargo-deny 0.20.2` and run `cargo deny --locked check licenses` when dependencies change. See [licensing](LICENSING.md).

## UI behavior

Moves use click-to-move: select a piece, then select a legal destination. Press Escape to clear the current selection or cancel a pending promotion. Keyboard shortcuts are Ctrl/Cmd+N for a new game, Ctrl/Cmd+Z to undo, and Ctrl/Cmd+F to flip the board. Submit FEN with Enter or the Load FEN button; Copy places the current FEN on the system clipboard.

## Platform status

The app has been launched on Debian 13 under native Wayland with Hyprland, and a mouse-driven `e2` to `e4` move was exercised. Full-workspace `cargo check --all-targets` passed for Windows GNU and macOS Darwin targets. Native linking, tests and runtime on those systems remain unverified; configured CI coverage is not a claim of supported platforms.
