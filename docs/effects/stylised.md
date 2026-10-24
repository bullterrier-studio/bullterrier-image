# Stylised

Looks that redraw the picture in a printing or drawing style, such as halftone.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/halftone.jpg" width="160" alt="halftone"> | `halftone` | Print-style dot screen: tone becomes dot size. | `cell=14` `angle=45.0` `ink=None` `ink_floor=0.35` `paper='#000000'` `paper_threshold=0.5` `sigma=0.5` `feather=1.0` `truncate=4.0` `edge='reflect'` `constant_value=0.0` | [halftone.toml](../../examples/effects/halftone.toml) |

### `halftone`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `cell` | whole number | px | 1 to 200 | `14` | Distance between neighbouring dots in px (the screen's pitch). Larger = fewer, bigger dots, a coarser print. |
| `angle` | number | degrees | -360 to 360 | `45.0` | Rotation of the dot grid in degrees, clockwise on screen: rows of dots run that many degrees below the x axis (negative = above). The grid is square, so 90 degrees on looks the same; 45 is the classic screen angle for one ink. |
| `ink` | colour or none |  |  | `None` | The ink: one colour for every dot ('#rrggbb', '#rgb' or three numbers 0..1); leave it out to give each dot the colour of the photo under it, brightened so small dots stay visible (by at most 1 / `ink_floor`). |
| `ink_floor` | number | fraction | 0.01 to 1 | `0.35` | With no `ink`: the most a dot's colour is brightened, as a floor 0.01 to 1 on its strongest channel: a colour whose strongest channel is below it is scaled by 1 / ink_floor instead of up to full, so dark areas stay dark. Lowering it brightens dark colours more; 1 = colours as they are. |
| `paper` | colour |  |  | `'#000000'` | The paper colour between the dots. On a dark paper (Rec. 709 luminance below `paper_threshold`) bright tones get the ink; on a light one, dark tones. |
| `paper_threshold` | number | fraction | 0 to 1 | `0.5` | The Rec. 709 luminance, 0 to 1, below which `paper` counts as dark (bright tones get the ink) rather than light (dark tones get it); 0.5 = mid grey. Raising it lets lighter paper count as dark: 0 treats every paper as light, 1 every paper but white as dark. |
| `sigma` | number | ratio | 0 to 5 | `0.5` | Gaussian sigma, in cells, of the blur that averages the tone each dot stands for: 0 = each dot sees only the pixel at its centre (detail, but noisy), larger = smoother tone that spreads into neighbouring dots. |
| `feather` | number | px | 0.1 to 50 | `1.0` | Width of each dot's soft edge in px (anti-aliasing): small = crisp dots, larger = blurrier dots. The ink stays the same. |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the tone blur's Gaussian is cut off, in sigmas: it reaches truncate x sigma cells. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'constant', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the tone blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant' (constant_value). |
| `constant_value` | number | fraction | 0 to 1 | `0.0` | The brightness (0..1) assumed beyond the edge when edge is 'constant'; ignored otherwise. |
