# Third-party notices

Chess Coach uses these separately licensed components:

- **shakmaty 0.30.2** — Niklas Fiekas and contributors; GPL-3.0-or-later. [Source](https://github.com/niklasf/shakmaty), [published license](https://docs.rs/crate/shakmaty/0.30.2/source/COPYING).
- **pgn-reader 0.29.1** — Niklas Fiekas and contributors; GPL-3.0-or-later. [Source](https://github.com/niklasf/shakmaty/tree/master/pgn-reader), [published license](https://docs.rs/crate/pgn-reader/0.29.1/source/COPYING).
- **rfd 0.17.2** — Marco I. Castaneda and contributors; MIT. Native file dialogs; on Linux the portal backend uses XDG Desktop Portal and may fall back to `zenity`. [Source and license](https://github.com/PolyMeilex/rfd/tree/0.17.2).
- **iced 0.14.0** — Héctor Ramón and contributors; MIT. [Source and license](https://github.com/iced-rs/iced/tree/0.14.0).
- **libc**, Unix only — Rust project contributors; MIT OR Apache-2.0. [Source](https://github.com/rust-lang/libc).
- **windows-sys**, Windows only — Microsoft and contributors; MIT OR Apache-2.0. [Source](https://github.com/microsoft/windows-rs).
- **DejaVu Sans 2.37** — Bitstream, DejaVu and Arev contributors. Unmodified font embedded for interface text. Full notices: [assets/fonts/LICENSE.txt](assets/fonts/LICENSE.txt). Upstream provenance and checksum: [licensing](docs/LICENSING.md).
- **Bundled SVG chess pieces** — 36 unchanged files from the Lichess `lila` repository, in Merida (Armando Hernandez Marroquin, GPL-2.0-or-later), Celtic (Maurizio Monge, MIT), and RhosGFX (CC0-1.0) sets. The [asset README](assets/pieces/README.md) records the fixed upstream revision, source URLs, and attribution; complete license texts and upstream licensing exceptions are preserved in `assets/pieces/`.
- **serde 1.0.229**, **serde_json 1.0.154**, **directories 6.0.0**, and **tempfile 3.27.0** — direct dependencies used for validated appearance settings and atomic file replacement; each is available under MIT OR Apache-2.0. Sources and complete notices are linked in the [locked dependency inventory](docs/DEPENDENCIES.md). `directories` pulls in **option-ext 0.2** under MPL-2.0; see the narrowly scoped exception and distribution notes in [licensing](docs/LICENSING.md).

See [the complete locked registry inventory](docs/DEPENDENCIES.md) for direct and transitive dependency versions and license expressions; each links to its published crate source. The project's license does not replace those licenses. Preserve all applicable notices when redistributing compiled applications; see [distribution notes](docs/LICENSING.md).

Stockfish is installed separately by the user and is **not included**. Its authors distribute it under GPLv3: [Stockfish source](https://github.com/official-stockfish/Stockfish) and [license](https://github.com/official-stockfish/Stockfish/blob/master/Copying.txt).
