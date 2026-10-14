# Application artwork

`icon.svg` is the Anki icon supplied by the project maintainer. It is retained
unchanged. `icon.png` is its 48×48 launcher version; `banner.png` uses the same
artwork. PNGs are committed so builds do not require an SVG renderer.

To regenerate the icon with CairoSVG 2.8.2:

```sh
cairosvg 3ds/app/resources/icon.svg -o 3ds/app/resources/icon.png -s 1.5
```

These assets do not imply endorsement by Ankitects.
