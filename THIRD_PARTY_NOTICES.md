# Third-party notices

Chess Coach uses these separately licensed components:

- **shakmaty 0.30.2** — Niklas Fiekas and contributors; GPL-3.0-or-later. [Source](https://github.com/niklasf/shakmaty), [published license](https://docs.rs/crate/shakmaty/0.30.2/source/COPYING).
- **pgn-reader 0.29.1** — Niklas Fiekas and contributors; GPL-3.0-or-later. [Source](https://github.com/niklasf/shakmaty/tree/master/pgn-reader), [published license](https://docs.rs/crate/pgn-reader/0.29.1/source/COPYING).
- **rfd 0.17.2** — Marco I. Castaneda and contributors; MIT. Native file dialogs; on Linux the portal backend uses XDG Desktop Portal and may fall back to `zenity`. [Source and license](https://github.com/PolyMeilex/rfd/tree/0.17.2).
- **iced 0.14.0** — Héctor Ramón and contributors; MIT. [Source and license](https://github.com/iced-rs/iced/tree/0.14.0).
- **libc**, Unix only — Rust project contributors; MIT OR Apache-2.0. [Source](https://github.com/rust-lang/libc).
- **windows-sys**, Windows only — Microsoft and contributors; MIT OR Apache-2.0. [Source](https://github.com/microsoft/windows-rs).
- **DejaVu Sans 2.37** — Bitstream, DejaVu and Arev contributors. Unmodified font embedded for the interface and Unicode chess pieces. Full notices: [assets/fonts/LICENSE.txt](assets/fonts/LICENSE.txt). Upstream provenance and checksum: [licensing](docs/LICENSING.md).

See [the complete locked registry inventory](docs/DEPENDENCIES.md) for direct and transitive dependency versions and license expressions; each links to its published crate source. The project's license does not replace those licenses. Preserve all applicable notices when redistributing compiled applications; see [distribution notes](docs/LICENSING.md).

Stockfish is installed separately by the user and is **not included**. Its authors distribute it under GPLv3: [Stockfish source](https://github.com/official-stockfish/Stockfish) and [license](https://github.com/official-stockfish/Stockfish/blob/master/Copying.txt).
