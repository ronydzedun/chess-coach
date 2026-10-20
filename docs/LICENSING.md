# Licensing

Project code is licensed under **GPL-3.0-or-later**: GNU GPL version 3, or (at your option) any later version. The complete GPLv3 text is in [LICENSE](../LICENSE); Rust sources carry SPDX headers. Contributions use the same terms. There is no contributor license agreement.

## Dependencies

Direct libraries were checked against published crate manifests and source:

| Component | Version | License | Minimum Rust |
| --- | --- | --- | --- |
| iced | 0.14.0 | MIT | 1.88 |
| shakmaty | 0.30.2 | GPL-3.0-or-later | 1.97 |
| pgn-reader | 0.29.1 | GPL-3.0-or-later | 1.97 |
| rfd | 0.17.2 | MIT | not declared; compiled with Rust 1.99.1 |
| serde | 1.0.229 | MIT OR Apache-2.0 | 1.56 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | 1.71 |
| directories | 6.0.0 | MIT OR Apache-2.0 | not declared |
| rusqlite | 0.40.2 | MIT | not declared in published manifest; compiled with Rust 1.99.1 |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | 1.63 |
| libc (Unix engine pipe polling) | see Cargo.lock | MIT OR Apache-2.0 | see crate manifest |
| windows-sys (Windows pipe polling) | see Cargo.lock | MIT OR Apache-2.0 | see crate manifest |

The application pins Rust 1.99.1. [DEPENDENCIES.md](DEPENDENCIES.md) inventories all resolved registry packages across targets, including their declared license expressions. `Cargo.lock` pins the actual sources/checksums. `deny.toml` defines the accepted license set and includes a narrow exception for `option-ext 0.2`, a transitive dependency of `directories` licensed under MPL-2.0. The project's source and dependency inventory were checked with cargo-deny; this automated source check is not a legal opinion and does not establish compliance for every future dependency update or distribution method. Repeat the check whenever dependencies change:

```sh
cargo install cargo-deny --version 0.20.2 --locked
cargo deny --locked check licenses
```

`rusqlite` is used with its `bundled` feature, which builds SQLite from the `libsqlite3-sys 0.38.2` source dependency rather than relying on a system SQLite library. SQLite itself is public domain; rusqlite and the bundled binding carry their own MIT notices. The application stores its local game library using SQLite; see [third-party notices](../THIRD_PARTY_NOTICES.md).

Permissive alternatives are available for dependencies declaring alternative licenses. For example, the allow-list accepts Apache-2.0 for a dependency offering `Apache-2.0 OR GPL-2.0-only`; it does not accept GPL-2.0-only by itself. The [FSF license list](https://www.gnu.org/licenses/license-list.html#apache2) discusses Apache-2.0 compatibility with GPLv3. The `option-ext` exception is limited to the crate actually pulled in by `directories`. The reviewed source has no Exhibit B notice identifying an incompatible secondary license. MPL-2.0 permits a larger work to be distributed under other terms while MPL-covered files remain under MPL; for binary distribution, retain notices and make the corresponding source for covered files available. Mozilla discusses MPL/GPL combinations in [FAQ question 14](https://www.mozilla.org/en-US/MPL/2.0/FAQ/#using-and-distributing-software-under-the-license). Review the specific combination and distribution method before shipping binaries. This repository's source check does not establish that all future combinations or distributions comply.

Before distributing compiled applications, retain required copyright/permission notices and assemble the relevant dependency license texts from the locked crate sources. For MPL-covered code, also make the corresponding source available under MPL-2.0 and preserve its notices; distributing this GPL-licensed larger work does not remove those obligations. This repository distributes application source and licensed visual assets; it does not currently publish installers or binary releases.

## Bundled visual assets

`assets/fonts/DejaVuSans.ttf` is the unmodified DejaVu Sans font from Debian 13's `fonts-dejavu-core` 2.37-8. SHA-256: `57f73e11f51999432bf7ab22ce55b6f945d5eca1bf824404cfa9ec2e3718c84e`. Upstream: [DejaVu Fonts](https://dejavu-fonts.github.io/). Its full [Bitstream Vera and Arev notices](../assets/fonts/LICENSE.txt) accompany the asset, including public-domain DejaVu modifications. The font is used by the interface; board pieces are SVGs, not Unicode glyphs.

The 36 bundled piece SVGs are unchanged copies from the fixed Lichess `lila` revision documented in [assets/pieces/README.md](../assets/pieces/README.md). The three sets have separate terms: Merida is GPL-2.0-or-later, Celtic is MIT, and RhosGFX is CC0-1.0. That README records upstream attribution and source links; full license texts and the upstream licensing exceptions are preserved alongside the artwork. These asset terms remain applicable and are not replaced by the project's GPL-3.0-or-later license. See [third-party notices](../THIRD_PARTY_NOTICES.md).

## Stockfish

Stockfish is an independently installed third-party component, distributed by its authors under GPLv3; see [upstream](https://github.com/official-stockfish/Stockfish) and its [license](https://github.com/official-stockfish/Stockfish/blob/master/Copying.txt). It is launched as an external process over UCI. No Stockfish binary or source is copied into this repository.

Launching a separate process does not automatically settle every licensing question. Before distributing installers that include Stockfish, review the exact combination and versions, corresponding-source obligations, notices, distribution method and any required source offer. Do not assume an upstream download link alone meets those obligations. There are no bundled-engine installers at this stage.
