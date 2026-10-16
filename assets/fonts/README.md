# Bundled fonts

NetDraw bundles regular upright fonts for canvas labels and embeds the selected font in exported SVG files. Inter is the default.

| Font | File | License | Source |
| --- | --- | --- | --- |
| Inter | `Inter-Regular.ttf` | SIL Open Font License 1.1 (`licenses/Inter-LICENSE`) | [Inter](https://github.com/rsms/inter) |
| Source Serif 4 | `SourceSerif4-Regular.otf` | SIL Open Font License 1.1 (`licenses/SourceSerif4-OFL.txt`) | [Adobe Source Serif](https://github.com/adobe-fonts/source-serif/tree/release) |
| IBM Plex Mono | `IBMPlexMono-Regular.ttf` | SIL Open Font License 1.1 (`licenses/IBMPlexMono-OFL.txt`) | [Google Fonts](https://github.com/google/fonts/tree/main/ofl/ibmplexmono) |
| Atkinson Hyperlegible | `AtkinsonHyperlegible-Regular.ttf` | SIL Open Font License 1.1 (`licenses/AtkinsonHyperlegible-OFL.txt`) | [Google Fonts](https://github.com/google/fonts/tree/main/ofl/atkinsonhyperlegible) |

The font files are redistributed unchanged under their accompanying licenses, which include copyright notices.

## Older drawings

Removed fonts are no longer bundled or offered in the picker. When opening an older `.netdraw` drawing, NetDraw substitutes fonts as follows:

| Previous font | Replacement |
| --- | --- |
| Noto Sans, Cantarell, Comfortaa, DejaVu Sans Condensed | Inter |
| Fira Mono | IBM Plex Mono |
| Latin Modern | Source Serif 4 |

Saving the drawing records the replacement font. Label size is preserved; text metrics and appearance may change.
