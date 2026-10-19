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
| libc (Unix engine pipe polling) | see Cargo.lock | MIT OR Apache-2.0 | see crate manifest |
| windows-sys (Windows pipe polling) | see Cargo.lock | MIT OR Apache-2.0 | see crate manifest |

The application pins Rust 1.99.1. [DEPENDENCIES.md](DEPENDENCIES.md) inventories all resolved registry packages across targets, including their declared license expressions. `Cargo.lock` pins the actual sources/checksums. `deny.toml` defines the accepted license set. The initial source tree passed `cargo-deny 0.20.2 --locked check licenses` without license exceptions or unknown licenses. Repeat it whenever dependencies change:

```sh
cargo install cargo-deny --version 0.20.2 --locked
cargo deny --locked check licenses
```

Permissive alternatives are available for dependencies declaring alternative licenses. For example, the allow-list accepts Apache-2.0 for a dependency offering `Apache-2.0 OR GPL-2.0-only`; it does not accept GPL-2.0-only by itself. The [FSF license list](https://www.gnu.org/licenses/license-list.html#apache2) discusses Apache-2.0 compatibility with GPLv3. A successful automated check is evidence about declared terms, not a legal opinion or a complete binary-distribution compliance review.

Before distributing compiled applications, retain required copyright/permission notices and assemble the relevant dependency license texts from the locked crate sources. This repository distributes application source and a licensed font; it does not currently publish installers or binary releases.

## Font

`assets/fonts/DejaVuSans.ttf` is the unmodified DejaVu Sans font from Debian 13's `fonts-dejavu-core` 2.37-8. SHA-256: `57f73e11f51999432bf7ab22ce55b6f945d5eca1bf824404cfa9ec2e3718c84e`. Upstream: [DejaVu Fonts](https://dejavu-fonts.github.io/). Its full [Bitstream Vera and Arev notices](../assets/fonts/LICENSE.txt) accompany the asset, including public-domain DejaVu modifications. Font terms remain separate from the application's GPL. No other third-party artwork is included.

## Stockfish

Stockfish is an independently installed third-party component, distributed by its authors under GPLv3; see [upstream](https://github.com/official-stockfish/Stockfish) and its [license](https://github.com/official-stockfish/Stockfish/blob/master/Copying.txt). It is launched as an external process over UCI. No Stockfish binary or source is copied into this repository.

Launching a separate process does not automatically settle every licensing question. Before distributing installers that include Stockfish, review the exact combination and versions, corresponding-source obligations, notices, distribution method and any required source offer. Do not assume an upstream download link alone meets those obligations. There are no bundled-engine installers at this stage.
