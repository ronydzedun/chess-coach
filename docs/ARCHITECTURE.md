# Architecture

Chess Coach is a Rust 2024 workspace with three crates:

```text
desktop ──> chess-core
   │
   └──────> engine
```

`desktop` provides the `chess-coach` binary and the `iced` 0.14 interface. It uses the `tiny-skia` software renderer with Wayland and X11 support. Board interaction is click-to-move: the app sends selected squares to `chess-core` and renders its state, legal destinations, recent move, and SAN history.

`chess-core` wraps `shakmaty` 0.30.2 for standard chess rules, FEN, UCI move parsing, and SAN generation. It owns the position, undo snapshots, move history, and a revision counter. A move or FEN load is validated before state changes, so rejected input leaves the position untouched. Loading valid FEN starts a fresh local history.

`engine` is a local UCI process service. Its worker owns process I/O and handshake waits, sends position and depth/time analysis requests, parses one principal variation in UCI coordinate notation, and reports centipawn or mate scores from White's perspective. Events carry request IDs and stale requests are cancelled. The service bounds queued events and cleans up its child process. The desktop interface lets users select an executable and analysis limit. It discovers Stockfish using `STOCKFISH_EXECUTABLE`, then `PATH`, then `/usr/games/stockfish`. No engine binary is bundled.

There is no PGN support, saved-game persistence, network service, or LLM integration.

## Process and UI lifecycle

The desktop is the only owner of `Game`. A 50 ms Iced/Tokio subscription drains engine events; no engine I/O runs on the GUI thread. A search captures both a unique request ID and the game's revision. Events must match both before they can update the visible evaluation. New moves, successful FEN loads, undo and reset invalidate analysis; invalid input preserves the valid game and its analysis. Flipping only changes presentation.

The engine service has one worker and one latest-request slot. Each search starts a fresh child process: `uci` → `uciok` → `isready` → `readyok` → `position fen …` → `go depth …` or `go movetime …`. Handshake waits expire after five seconds each. Search results are normalized from side-to-move scores to White's perspective. `bestmove` completes the search. UCI coordinates remain separate from the game's SAN history.

`stop` invalidates the generation immediately; cancellation and `Drop` send stop/quit, allow a short grace period, then kill and reap an unresponsive child. The bounded reader uses Unix polling or Windows `PeekNamedPipe`, so inherited stdout does not prevent shutdown. Full queues may discard intermediate `info`, while terminal protocol messages are preserved. Search, line, PV and queue sizes have bounds. Depth searches have a 60-second safety deadline; time searches accept up to five minutes.

`Engine::drop` joins its worker and reader after cleanup. There is no detached child service. The engine is optional and is not spawned during ordinary application startup. Future game review can build on `Game` and the UCI service; persistence, training and a coach should be introduced only when they have real implementations.
