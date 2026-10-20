# Development

## Requirements

- Rust 1.99.1, installed through `rustup` (version pinned in `rust-toolchain.toml`), with `rustfmt` and `clippy`.
- A C toolchain and `pkg-config`.
- Native dependencies for the selected display backends. On Debian 13:

  ```sh
  sudo apt install build-essential pkg-config libwayland-dev libxkbcommon-dev \
    libx11-dev libxcursor-dev libxi-dev libxrandr-dev libxkbcommon-x11-0
  ```

PGN file selection uses `rfd`'s native dialogs. On Linux, the dialog tries XDG Desktop Portal and can fall back to `zenity`; install a portal implementation or `zenity` if the file picker is unavailable. Windows and macOS use their native dialogs.

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

The chess rules and ordinary workspace tests do not need Stockfish. The `engine` crate launches a local UCI executable only when analysis is requested. Set `STOCKFISH_EXECUTABLE` to select a binary explicitly; otherwise discovery checks `PATH`, then `/usr/games/stockfish`. The real-engine test runs when `STOCKFISH_EXECUTABLE` is set and reports a skip otherwise. Portable fake UCI processes exercise handshake, advertised resource limits, process reuse, cancellation and late-result barriers, reset, idle exits, flooding, and active/idle cleanup on every platform:

```sh
STOCKFISH_EXECUTABLE=/usr/games/stockfish cargo test -p engine --locked -- --nocapture
STOCKFISH_EXECUTABLE=/usr/games/stockfish cargo test -p desktop long_game_stockfish_opt_in -- --nocapture
```

Install `cargo-deny 0.20.2` and run `cargo deny --locked check licenses` when dependencies change. See [licensing](LICENSING.md).

## UI behavior

Moves use click-to-move: select a piece, then select a legal destination. Press Escape to clear the current selection or cancel a pending promotion. Keyboard shortcuts are Ctrl/Cmd+O for PGN import, Ctrl/Cmd+N for a new game, Ctrl/Cmd+Z to undo, and Ctrl/Cmd+F to flip the board. Submit FEN with Enter or the Load FEN button; Copy places the current FEN on the system clipboard.

PGN review accepts pasted text or a selected file. File reading and parsing happen in a background worker. An import is atomic and bounded to 8 MiB, 256 games, 2,048 plies per game, and 50,000 plies total; a failed or cancelled import leaves the current board intact. It validates standard-chess main lines, SAN, `SetUp`/`FEN`, and game results. It accepts UTF-8 with an optional BOM, comments, and NAGs; it rejects unsupported variants and malformed or illegal moves. Navigation keys Left/Right/Home/End work when focus is outside the text editor. Undo retains the forward working line until a new move branches it; the original imported line remains available to reload. Source PGN is not changed.

The original PGN text and game IDs are stored in the local SQLite library. Identical PGN text deduplicates against the existing import. The database connection lives on a dedicated worker thread. Completed imported-game analyses are stored by game, exact position/history and engine configuration, and loaded at startup; pending work resumes automatically. The active original game and cursor are restored. Exploratory working branches and provisional graph previews are not persisted. The database is stored at `ProjectDirs("org", "chess-coach", "chess-coach").data_local_dir()/library.sqlite3`; on Linux this is normally `~/.local/share/chess-coach/library.sqlite3`, subject to `XDG_DATA_HOME`. SQLite WAL may leave `-wal` and `-shm` files beside it, so close the app before copying the database for backup. Unrecognized, newer or corrupt databases report an error and are preserved without destructive automatic migration.

Successful import selects the Moves tab; Analysis opens the engine controls for the current position. Tab changes do not invalidate analysis. Supplied check/mate SAN suffixes must match the board (omitted suffixes are accepted). Explicit results cannot contradict a checkmate or stalemate; `*` remains valid for analysis positions.

When the requested limit is above 200 ms or depth 8, game analysis first runs a quick pass at 200 ms or depth 8, then analyzes every position at the requested limit. The current position is prioritized; large evaluation swings and mate changes from the preview are prioritized in the second pass. The report graph can show provisional preview values, which final results replace. The progress percentage counts both work units per position in two-pass mode and reports quick-pass progress separately.

The current review scope does not include variation review or editing, comment display, PGN export, graph-based move classification, training, or AI coaching.

## PGN UI smoke check

1. Run the app and choose Import PGN → Open PGN file. Select `crates/chess-core/tests/fixtures/synthetic_chess_com.pgn`, or paste its contents and import.
2. Switch between the two games. Use Start/Back/Next/End, arrow keys, and the move list; the complete line should remain available.
3. Go to an earlier position and play another legal move. Reload original to restore the imported line.
4. Paste `1.e4 e5 2.Bh6 *`. Import must report an illegal move; Cancel must return to the previous position.
5. Analyze an imported position with Stockfish, then navigate away. The old score/PV must clear immediately.

## Platform status

The app has been launched on Debian 13 under native Wayland with Hyprland, and a mouse-driven `e2` to `e4` move was exercised. The SQLite library milestone was exercised on Debian 13 via X11/Xvfb: two imports and their completed scores survived restart without new engine searches; an interrupted third import resumed remaining work, and time/depth preferences plus cursor were restored. CI checks native builds, tests, Clippy and formatting on the Linux, Windows and macOS runners; consult the [current CI results](https://github.com/ronydzedun/chess-coach/actions/workflows/ci.yml) for the revision being tested. Graphical runtime on Windows/macOS remains unverified.

## GUI responsiveness checks

The dev profile optimizes `tiny-skia`, `tiny-skia-path` and `iced_tiny_skia` at level 2. Their unoptimized software rasterization can make selection highlights noticeably slower than subsequent pointer motion. Application crates remain unoptimized for fast iteration and debugging; debug assertions and overflow checks retain their normal defaults. Changing these overrides may recompile renderer dependencies. For distribution builds use `cargo build --release --workspace --locked`.

When checking drag responsiveness, start from an unselected piece and press/move immediately, without a settling delay after mouse-down. Compare with the same piece already selected. Verify grab offsets, selection hints, clicks/double clicks, capture, promotion, outside drops, Escape, flip and resize, both with analysis enabled and without an installed engine. Automated widget tests cover redraw-only press/motion and render-slot stability, but do not establish native display latency; record the display backend used for manual measurements.

The desktop opt-in test analyzes a 300-ply synthetic imported game while repeatedly navigating, then revisits all 301 positions to verify completion and retention beyond the 256-entry general cache. It uses a 10 ms search limit for bounded test runtime; this is not a strength benchmark.
