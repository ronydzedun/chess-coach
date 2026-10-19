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

`engine` is a local UCI process service. Its worker owns process I/O and handshake waits, sends position and depth/time analysis requests, parses one principal variation in UCI coordinate notation, and reports centipawn or mate scores from White's perspective. Events carry request IDs and stale requests are cancelled. The service bounds queued events and cleans up its child process. A saved explicit engine path takes priority. Otherwise the desktop interface discovers Stockfish using `STOCKFISH_EXECUTABLE`, then `PATH`, then `/usr/games/stockfish`. No engine binary is bundled. Automatic analysis is ON at every launch and can be toggled ON/OFF for the current session. It begins at startup and is debounced by 300 ms after position, FEN, import, and review-navigation changes. The time limit defaults to 1000 ms and accepts 1–300000 ms; depth accepts 1–128 and retains a 60-second safety deadline. Changing engine path or limit restarts pending automatic analysis. Engine errors do not create a retry loop.

PGN source text and parsed games are held in memory only. The original imported lines remain available while the user navigates or makes moves on a separate working line; reloading a game restores its original line. There is no PGN export, saved-game persistence, network service, or LLM integration.

## Process and UI lifecycle

The desktop owns the active `Game`, the retained imported source, and the selected original game. A 50 ms Iced/Tokio subscription drains engine events; no engine I/O runs on the GUI thread. File reading and PGN parsing run off the GUI thread. A search captures both a unique request ID and the game's revision. Every position change cancels analysis, and events must match both before they can update the visible evaluation. Moves, review navigation, successful FEN loads, undo and reset invalidate analysis; failed or cancelled imports preserve the current board. Flipping only changes presentation. The board's adjacent vertical evaluation bar is white-relative: centipawns map through a bounded tanh scale with 400 cp as its scale factor, so the bar is not a win-probability estimate. Unknown scores are neutral, mate scores saturate at either end, and M0 marks the winner.

Undo in review is non-destructive: it moves the cursor backward while retaining the forward working line. Advancing restores those moves. A new move after going backward truncates only the future of the working line; the retained imported original remains available through reload or game selection. Left/Right navigate one ply and Home/End jump to the line boundaries when a text editor does not own keyboard focus. The native file picker uses `rfd` 0.17.2; on Linux it uses XDG Desktop Portal with a `zenity` fallback, and on Windows/macOS it uses native dialogs.

The engine service has one worker and one latest-request slot. Each search starts a fresh child process: `uci` → `uciok` → `isready` → `readyok` → `position fen …` → `go depth …` or `go movetime …`. Handshake waits expire after five seconds each. Search results are normalized from side-to-move scores to White's perspective. `bestmove` completes the search. UCI coordinates remain separate from the game's SAN history.

`stop` invalidates the generation immediately; cancellation and `Drop` send stop/quit, allow a short grace period, then kill and reap an unresponsive child. The bounded reader uses Unix polling or Windows `PeekNamedPipe`, so inherited stdout does not prevent shutdown. Full queues may discard intermediate `info`, while terminal protocol messages are preserved. Search, line, PV and queue sizes have bounds. Depth searches have a 60-second safety deadline; time searches accept up to five minutes.

`Engine::drop` joins its worker and reader after cleanup. There is no detached child service. The engine is optional; if no executable is found, the board remains usable and automatic analysis reports the issue without retrying. Appearance preferences are persisted separately in `appearance.json` via `directories::ProjectDirs::from("org", "chess-coach", "chess-coach")`. This resolves to `$XDG_CONFIG_HOME/chess-coach` (normally `~/.config/chess-coach`) on Linux, `~/Library/Application Support/org.chess-coach.chess-coach` on macOS, and `%APPDATA%\chess-coach\chess-coach\config` on Windows. The JSON file is limited to 64 KiB and supports at most 32 custom palettes with unique names of at most 40 characters and distinct light/dark `#RRGGBB` colors. Saves use a same-directory temporary file and atomic replacement. Invalid settings fall back to defaults and the error is shown in the footer and the Appearance section of Settings. Three bundled SVG sets are Merida, Celtic and RhosGFX; four built-in palettes are Classic, Green, Blue and Slate. Custom palette colors and the selected piece set/palette are saved. DejaVu Sans is used by the UI; chess pieces are SVG artwork, not Unicode glyphs. Persistence for games, training and a coach remain future work.


## Settings dialog

The Settings dialog is a modal Iced overlay with Engine and Appearance sections. Pointer events and board shortcuts are blocked while it is open; engine polling continues. Engine path edits are a draft until Apply. An invalid path or failed save preserves the active engine and the previous configuration. Apply saves first, then changes the engine, cancelling stale analysis if necessary. Closing, Cancel, Escape and backdrop clicks discard the path draft. Appearance changes still apply and save immediately. `engine_path` is an optional field in the existing `appearance.json`; older appearance files remain compatible. A saved custom path overrides discovery. Clearing it restores discovery through `STOCKFISH_EXECUTABLE`, PATH and `/usr/games/stockfish`.

Legal-move markers are filled circles for empty destinations and softly tinted squares for captures. Hover and check indications use background shading; no square borders are drawn.
