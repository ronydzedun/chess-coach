# Bundled chess piece artwork

All 36 SVG files are unmodified copies (two colors × six roles × three sets),
downloaded 2026-10-19 from the Lichess repository at the fixed revision
`d4b7619db091e67197b5f9be9e33549b9e08013f`:

https://github.com/lichess-org/lila/tree/d4b7619db091e67197b5f9be9e33549b9e08013f/public/piece

The licensing exceptions explicitly covering these directories and their authors
are preserved verbatim in [UPSTREAM-COPYING.md](UPSTREAM-COPYING.md), fetched from:
https://github.com/lichess-org/lila/blob/d4b7619db091e67197b5f9be9e33549b9e08013f/COPYING.md

| Directory | Artist | License | Complete license |
| --- | --- | --- | --- |
| `merida` | Armando Hernandez Marroquin | GPL-2.0-or-later | [GPL-2.0.txt](GPL-2.0.txt) |
| `celtic` | Maurizio Monge | MIT | [MIT-Celtic.txt](MIT-Celtic.txt) |
| `rhosgfx` | RhosGFX | CC0-1.0 | [CC0-1.0.txt](CC0-1.0.txt) |

Source URLs for each unchanged SVG follow the exact pattern:
`https://raw.githubusercontent.com/lichess-org/lila/d4b7619db091e67197b5f9be9e33549b9e08013f/public/piece/{directory}/{color}{role}.svg`,
where color is `w` or `b`, and role is `P`, `N`, `B`, `R`, `Q`, or `K`.
The distributed SVGs themselves are the editable source artwork.

The Celtic license includes the artist's copyright notice, copied from the
artist's own repository at fixed revision
`d8705d8bc9879fef44208de19e2a20c937409e1f`:
https://github.com/maurimo/chess-art/blob/d8705d8bc9879fef44208de19e2a20c937409e1f/LICENSE

Other full license texts were fetched from their publishers:

- GPL version 2: https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt
- CC0 version 1.0: https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt

`SHA256SUMS` records the downloaded artwork, upstream attribution snapshot, and
license text hashes. No runtime network request is required to use the pieces.
The repository's GPL-3.0-or-later license does not replace these artwork licenses.
