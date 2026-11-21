# Brand assets

Source artwork for the Notwig mark.

| File | Use |
| --- | --- |
| `notwig.svg` | Original mark. |
| `notwig-app-icon.svg` / `.png` | Native application icon: black mark on light blue. |
| `notwig-dark.svg` / `.png` | White mark for dark in-app surfaces. |
| `notwig-light.svg` / `.png` | Black mark for light in-app surfaces. |

`notwig-ui` embeds the dark and light PNGs at compile time and draws the variant
matching the current theme left of the FILE button. `notwig-app` embeds the app
icon PNG for the native window and dock icon.

Regenerate the PNGs after changing an SVG:

```sh
./scripts/build-brand-assets.sh
```

The PNGs are committed so builds do not require an SVG rasterizer.
