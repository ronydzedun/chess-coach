# Architecture

Chess Coach is a Rust 2024 workspace with three crates:

```text
desktop ──> chess-core
   │
   └──────> engine
```

`desktop` provides the `chess-coach` binary and the `iced` 0.14 interface. It uses the `tiny-skia` software renderer with Wayland and X11 support. Board interaction is click-to-move: the app sends selected squares to `chess-core` and renders its state, legal destinations, recent move, and SAN history.

`chess-core` wraps `shakmaty` 0.30.2 for standard chess rules, FEN, UCI move parsing, and SAN generation. It owns the position, undo snapshots, move history, and a revision counter. A move or FEN load is validated before state changes, so rejected input leaves the position untouched. Loading valid FEN starts a fresh local history. Review lines can be loaded as independent validated histories while preserving the revision sequence used to reject stale analysis.

PGN import uses `pgn-reader` 0.29.1 and validates the main line against `shakmaty`. The parser accepts standard-chess games, UTF-8 (with an optional BOM), comments, NAGs, and `SetUp`/`FEN` tags. It rejects malformed structure, illegal or unsupported SAN, unsupported variants, invalid FEN, and missing game results. An import is committed only when every game parses successfully. Input is limited to 8 MiB, 256 games, 2,048 plies per game, and 50,000 plies total. Variations are not retained for review and comments are not shown.

`engine` is a local UCI process service. Its worker owns process I/O and handshake waits, sends position and depth/time analysis requests, parses one principal variation in UCI coordinate notation, and reports centipawn or mate scores from White's perspective. Events carry request IDs and stale requests are cancelled. The service bounds queued events and cleans up its child process. The desktop interface lets users select an executable and analysis limit. It discovers Stockfish using `STOCKFISH_EXECUTABLE`, then `PATH`, then `/usr/games/stockfish`. No engine binary is bundled.

PGN source text and parsed games are held in memory only. The original imported lines remain available while the user navigates or makes moves on a separate working line; reloading a game restores its original line. There is no PGN export, saved-game persistence, network service, or LLM integration.

## Process and UI lifecycle

The desktop owns the active `Game`, the retained imported source, and the selected original game. A 50 ms Iced/Tokio subscription drains engine events; no engine I/O runs on the GUI thread. File reading and PGN parsing run off the GUI thread. A search captures both a unique request ID and the game's revision. Every position change cancels analysis, and events must match both before they can update the visible evaluation. Moves, review navigation, successful FEN loads, undo and reset invalidate analysis; failed or cancelled imports preserve the current board. Flipping only changes presentation.

Undo in review is non-destructive: it moves the cursor backward while retaining the forward working line. Advancing restores those moves. A new move after going backward truncates only the future of the working line; the retained imported original remains available through reload or game selection. Left/Right navigate one ply and Home/End jump to the line boundaries when a text editor does not own keyboard focus. The native file picker uses `rfd` 0.17.2; on Linux it uses XDG Desktop Portal with a `zenity` fallback, and on Windows/macOS it uses native dialogs.

The engine service has one worker and one latest-request slot. Each search starts a fresh child process: `uci` → `uciok` → `isready` → `readyok` → `position fen …` → `go depth …` or `go movetime …`. Handshake waits expire after five seconds each. Search results are normalized from side-to-move scores to White's perspective. `bestmove` completes the search. UCI coordinates remain separate from the game's SAN history.

`stop` invalidates the generation immediately; cancellation and `Drop` send stop/quit, allow a short grace period, then kill and reap an unresponsive child. The bounded reader uses Unix polling or Windows `PeekNamedPipe`, so inherited stdout does not prevent shutdown. Full queues may discard intermediate `info`, while terminal protocol messages are preserved. Search, line, PV and queue sizes have bounds. Depth searches have a 60-second safety deadline; time searches accept up to five minutes.

`Engine::drop` joins its worker and reader after cleanup. There is no detached child service. The engine is optional and is not spawned during ordinary application startup. Persistence, training and a coach remain future work.
