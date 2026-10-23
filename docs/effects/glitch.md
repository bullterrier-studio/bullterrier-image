# Glitch

Digital-error looks: displaced slices, colour splits and corrupted blocks.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/glitch.jpg" width="160" alt="glitch"> | `glitch` | `bands` random horizontal strips slide sideways by up to `shift` of the width, then red and blue are pushed `split` px apart. | `split=10` `bands=7` `shift=0.06` `ascii_share=0.0` `ascii_cell=9` `ascii_photo=0.0` `seed=0` `band_min_height=0.01` `band_max_height=0.06` `ascii_rows=3` `ascii_gamma=0.55` `ascii_chars=' .:-=+*#%@'` `ascii_ink=None` `ascii_ink_floor=0.35` `ascii_paper='#000000'` `ascii_paper_threshold=0.5` `ascii_font=None` | [glitch.toml](../../examples/effects/glitch.toml) |

### `glitch`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `split` | whole number | px | -500 to 500 | `10` | How far red is pushed right and blue left, -500 to 500 px (negative swaps the directions); 0 = no colour split. The picture wraps around at the edges. |
| `bands` | whole number | count | 0 to 500 | `7` | How many random horizontal strips slide sideways, 0 to 500; 0 = none. |
| `shift` | number | fraction | 0 to 1 | `0.06` | The furthest a strip slides, as a fraction 0 to 1 of the picture's width; each strip slides a random distance up to this, left or right, wrapping around. |
| `ascii_share` | number | fraction | 0 to 1 | `0.0` | The fraction 0 to 1 of the strips that also dissolve into text (rounded up to whole strips); 0 = no text. |
| `ascii_cell` | whole number | px | 1 to 200 | `9` | Width of one character of the text strips, 1 to 200 px. Larger = bigger, fewer characters. |
| `ascii_photo` | number | fraction | 0 to 1 | `0.0` | How much of the strip shows through behind its characters, 0 to 1: 0 = text on flat `ascii_paper`, 1 = text laid over the strip, keeping all its detail. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed: the same seed always gives the same damage; any other gives different strips. |
| `band_min_height` | number | fraction | 0 to 1 | `0.01` | The lowest a strip can be, as a fraction 0 to 1 of the picture's height (never below 1 px). Each strip's height is drawn at random between this and `band_max_height`, which it may not exceed. |
| `band_max_height` | number | fraction | 0 to 1 | `0.06` | The tallest a strip can be, as a fraction 0 to 1 of the picture's height. Raising it gives taller strips. |
| `ascii_rows` | whole number | count | 1 to 100 | `3` | The fewest rows of characters a text strip holds, 1 to 100: a thinner strip is widened (each row counted as twice `ascii_cell` tall) so its text stays legible, up to the whole picture. |
| `ascii_gamma` | number | ratio | 0.1 to 10 | `0.55` | Exponent on the amount of ink each character puts down, 0.1 to 10: below 1 (the default) more ink, so the text carries enough light to read against dark paper; above 1 less. |
| `ascii_chars` | text |  |  | `' .:-=+*#%@'` | The character palette of the text strips, in any order, at least two different characters; sorted by how much ink each glyph puts down, so the densest is used for the brightest areas on dark paper and the darkest on light paper (see `ascii_paper`). |
| `ascii_ink` | colour or none |  |  | `None` | The ink colour of the text strips ('#rrggbb', '#rgb' or [r, g, b] 0..1); none (the default) colours each character from the average colour of the strip under it, brightened as in the ascii effect (strongest channel scaled to full, by at most 1 / `ascii_ink_floor`). |
| `ascii_ink_floor` | number | fraction | 0.01 to 1 | `0.35` | With no `ascii_ink`: the most a character's colour is brightened, as a floor 0.01 to 1 on its strongest channel: a colour whose strongest channel is below it is scaled by 1 / ascii_ink_floor instead of up to full, so dark strips stay dark. Lowering it brightens dark colours more; 1 = colours as they are. |
| `ascii_paper` | colour |  |  | `'#000000'` | The paper colour behind the text strips' characters. Dark paper (Rec. 709 luminance below `ascii_paper_threshold`) puts dense characters on bright areas; lighter paper puts them on dark areas. |
| `ascii_paper_threshold` | number | fraction | 0 to 1 | `0.5` | The Rec. 709 luminance, 0 to 1, below which `ascii_paper` counts as dark (dense characters on bright areas) rather than light (dense characters on dark areas); 0.5 = mid grey. Raising it lets lighter paper count as dark: 0 treats every paper as light, 1 every paper but white as dark. |
| `ascii_font` | text or none |  |  | `None` | Path to a monospaced .ttf/.ttc font for the text strips; none (the default) uses the first system monospaced font found. |
