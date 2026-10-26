# Glitch

Digital-error looks: displaced slices, colour splits and corrupted blocks.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/anaglyph.jpg" width="160" alt="anaglyph"> | `anaglyph` | Red from the brightness moved `shift` px left, green and blue from the brightness moved `shift` px right. | `shift=6.0` +1 advanced | [anaglyph.toml](../../examples/effects/anaglyph.toml) |
| <img src="../gallery/dead_pixels.jpg" width="160" alt="dead_pixels"> | `dead_pixels` | Make `count` cells of `size` px faulty: a share `stuck` of them shows `color`, the rest black. | `count=120` `size=4` `stuck=0.5` `color='#ffffff'` `seed=0` | [dead_pixels.toml](../../examples/effects/dead_pixels.toml) |
| <img src="../gallery/echo.jpg" width="160" alt="echo"> | `echo` | Lay `copies` translucent copies over the picture, each (`dx`, `dy`) px further from it and weaker by `fade`. | `copies=4` `dx=14.0` `dy=0.0` `fade=0.5` +1 advanced | [echo.toml](../../examples/effects/echo.toml) |
| <img src="../gallery/glitch.jpg" width="160" alt="glitch"> | `glitch` | `bands` random horizontal strips slide sideways by up to `shift` of the width, then red and blue are pushed `split` px apart. | `split=10` `bands=7` `shift=0.06` `seed=0` `band_min_height=0.01` `band_max_height=0.06` | [glitch.toml](../../examples/effects/glitch.toml) |
| <img src="../gallery/hologram.jpg" width="160" alt="hologram"> | `hologram` | Show the picture as light of `color`, outlined where its brightness changes, with scanlines and a flicker. | `color='#30e0ff'` `tint=0.85` `glow=5.0` `scanlines=0.35` `flicker=0.3` `phase=0.0` +5 advanced | [hologram.toml](../../examples/effects/hologram.toml) |
| <img src="../gallery/interlace.jpg" width="160" alt="interlace"> | `interlace` | Slide every other row of the picture sideways by `offset` px, leaving the rows between untouched. | `offset=6.0` `field='odd'` +1 advanced | [interlace.toml](../../examples/effects/interlace.toml) |
| <img src="../gallery/jpeg_crush.jpg" width="160" alt="jpeg_crush"> | `jpeg_crush` | Compress the picture with a JPEG codec written as maths at `quality`, damage `hits` coefficients, and decode it. | `quality=6` `hits=0` `seed=0` +2 advanced | [jpeg_crush.toml](../../examples/effects/jpeg_crush.toml) |
| <img src="../gallery/ntsc.jpg" width="160" alt="ntsc"> | `ntsc` | Smear the colour of the picture along its rows and let a share of it crawl into brightness, like an NTSC signal. | `sigma=4.0` `crawl=0.3` `phase=0.0` +2 advanced | [ntsc.toml](../../examples/effects/ntsc.toml) |
| <img src="../gallery/rgb_split.jpg" width="160" alt="rgb_split"> | `rgb_split` | Push the red and blue channels `amount` px apart along `angle`, leaving green where it is. | `angle=30.0` `amount=8.0` +1 advanced | [rgb_split.toml](../../examples/effects/rgb_split.toml) |
| <img src="../gallery/scan_roll.jpg" width="160" alt="scan_roll"> | `scan_roll` | Roll the picture by `offset` of its height (`axis` 'vertical') or width ('horizontal'), as a slipped sync does. | `axis='vertical'` `offset=0.2` +1 advanced | [scan_roll.toml](../../examples/effects/scan_roll.toml) |
| <img src="../gallery/signal_noise.jpg" width="160" alt="signal_noise"> | `signal_noise` | Mix random grey static into `bands` full-width bands, then stamp `dropouts` short streaks of `color` on top. | `bands=3` `band_height=0.04` `static=0.7` `dropouts=12` `color='#ffffff'` `seed=0` +2 advanced | [signal_noise.toml](../../examples/effects/signal_noise.toml) |
| <img src="../gallery/vhs.jpg" width="160" alt="vhs"> | `vhs` | Make the picture look like a worn videotape: tracking bands, wobbling rows, bled colour and tape noise. | `tracking=0.06` `bands=3` `bleed=5.0` `noise=0.03` `wobble=1.5` `seed=0` +5 advanced | [vhs.toml](../../examples/effects/vhs.toml) |

### `anaglyph`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `shift` | number | px | 0 to 200 | `6.0` | How far each eye's view is moved from the picture, in px tuned for a picture about 480 px high: the red (left) view moves this far to the left and the cyan (right) view this far to the right, so they end up twice this far apart and objects seem to float out of the screen through red-cyan glasses. 0 gives a grey picture, the brightness in all three channels. A fraction of a pixel blends the two pixels it lies between. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'nearest', 'wrap', 'reflect', 'mirror' |  |  | `'nearest'` | What a moved view shows where it has left the picture's edge: 'nearest' repeats the view's edge pixels (a coloured smear along the edge), 'wrap' brings back what left on the other side, 'reflect' mirrors the view (the edge line repeated once), 'mirror' mirrors it about the edge line. |

</details>

### `dead_pixels`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `count` | whole number | count | 0 to 100000 | `120` | How many faulty pixels there are, each a square of `size` px: 0 leaves the picture unchanged. They lie on a grid of `size` px cells, so none overlaps another, and each is a different one. A count larger than the number of cells fills every cell. Raising it adds faulty pixels and never moves one already there. |
| `size` | whole number | px | 1 to 64 | `4` | The width and height in px of one faulty pixel, tuned for a picture about 480 px high: 1 is a single pixel of the picture, larger is a screen with bigger pixels, a block at the edge being cut off by it. |
| `stuck` | number | fraction | 0 to 1 | `0.5` | The share of the faulty pixels that are stuck on `color`; the rest are dead and black. 0 = all dead, 1 = all stuck. The number stuck is `count` x `stuck`, rounded to a whole pixel. |
| `color` | colour |  |  | `'#ffffff'` | The colour a stuck pixel shows whatever the picture is under it: '#rrggbb', '#rgb' or [r, g, b] 0..1. White is the usual fault; a red, green or blue is one stuck sub-pixel. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Chooses where the faulty pixels are and which of them are stuck; the same seed and count give the same picture, another seed another one. |

### `echo`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `copies` | whole number | count | 0 to 32 | `4` | How many copies are laid over the picture, 0 to 32; 0 leaves the picture unchanged. More copies make a longer trail (the time grows with the count). |
| `dx` | number | px | -500 to 500 | `14.0` | How far each copy lies to the right of the one before it, in pixels, -500 to 500 (negative is to the left); copy k is k times this far from the picture. A fraction of a pixel blends the two pixels it lies between. |
| `dy` | number | px | -500 to 500 | `0.0` | How far each copy lies below the one before it, in pixels, -500 to 500 (negative is above); copy k is k times this far from the picture. With dx and dy both 0 the copies lie on the picture and change nothing. |
| `fade` | number | fraction | 0 to 1 | `0.5` | How much weaker each copy is than the one before it, 0 to 1: the original counts 1, copy k counts (1 - fade) to the power k. 0 gives every copy the strength of the original; 0.5 halves each; 1 makes the copies vanish and leaves the picture unchanged. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'nearest', 'wrap', 'reflect', 'mirror' |  |  | `'nearest'` | What a copy shows where it has moved off the picture's edge: 'nearest' repeats the picture's edge pixels (a smear along the edge), 'wrap' brings back what left on the other side, 'reflect' mirrors the picture (the edge line repeated once), 'mirror' mirrors it about the edge line. |

</details>

### `glitch`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `split` | whole number | px | -500 to 500 | `10` | How far red is pushed right and blue left, -500 to 500 px (negative swaps the directions); 0 = no colour split. The picture wraps around at the edges. |
| `bands` | whole number | count | 0 to 500 | `7` | How many random horizontal strips slide sideways, 0 to 500; 0 = none. |
| `shift` | number | fraction | 0 to 1 | `0.06` | The furthest a strip slides, as a fraction 0 to 1 of the picture's width; each strip slides a random distance up to this, left or right, wrapping around. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed: the same seed always gives the same damage; any other gives different strips. |
| `band_min_height` | number | fraction | 0 to 1 | `0.01` | The lowest a strip can be, as a fraction 0 to 1 of the picture's height (never below 1 px). Each strip's height is drawn at random between this and `band_max_height`, which it may not exceed. |
| `band_max_height` | number | fraction | 0 to 1 | `0.06` | The tallest a strip can be, as a fraction 0 to 1 of the picture's height. Raising it gives taller strips. |

### `hologram`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `color` | colour |  |  | `'#30e0ff'` | The colour of the projected light, which the picture's brightness and its edges are shown in: '#rrggbb', '#rgb' or [r, g, b] 0..1. A cyan is the classic hologram. |
| `tint` | number | fraction | 0 to 1 | `0.85` | How far the picture is replaced by its brightness as light of `color`: 0 = the picture's own colours, 1 = only the colour, bright where the picture is bright and dark where it is dark. |
| `glow` | number | ratio | 0 to 50 | `5.0` | How brightly the edges shine: the steepness of the picture's brightness (its change per pixel, about 0.1 at a clear edge) times this is the light of `color` screened over the picture there, so it lights both sides of an edge. 0 = no edge light, and a flat area never has any; raising it makes edges brighter and more of the weaker ones show. |
| `scanlines` | number | fraction | 0 to 1 | `0.35` | How dark the gaps between the scanlines are: the picture is darkened between the lines by up to this share, smoothly, so a line is brightest in its middle. 0 = no scanlines, 1 = black between them. |
| `flicker` | number | fraction | 0 to 1 | `0.3` | How dim the dimmest rows of the flicker are: bands of rows are darkened, smoothly, by up to this share, and `phase` says where on the picture they are. 0 = no flicker, 1 = black at the dimmest. |
| `phase` | number | cycles | 0 to 1 | `0.0` | Where the flicker's bands are in their cycle: they roll down the picture by their own height over one cycle, so 0.25 is a quarter of the way and 1 is back at 0. It only matters when `flicker` is above 0. |

<details><summary>Advanced parameters (5)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `scanline_spacing` | number | px | 2 to 32 | `3.0` | The distance in px from one scanline to the next, tuned for a picture about 480 px high: lines are centred on rows 0, spacing, 2 x spacing... and the gaps between them lie half a spacing away. A larger spacing is fewer, wider-set lines. |
| `flicker_period` | number | px | 2 to 2000 | `160.0` | The height in px of one band of the flicker, from one dim row to the next, tuned for a picture about 480 px high: the flicker repeats every this many rows. A larger period is fewer, wider bands. |
| `edge_sigma` | number | px | 0 to 20 | `1.5` | Gaussian sigma in px of the smoothing the edges are found after, tuned for a picture about 480 px high: larger ignores fine detail and noise and gives wider, softer outlines; 0 finds every pixel's edge. |
| `truncate` | number | ratio | 1 to 10 | `4.0` | Where the edge smoothing's Gaussian is cut off, in sigmas: it reaches truncate x `edge_sigma` px. Larger is more exact and slower. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'nearest'` | What the edge finding sees beyond the picture's border: 'nearest' (the edge pixel repeated, so the border is no edge), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'wrap' (the opposite edge, so a different brightness there makes an edge). |

</details>

### `interlace`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `offset` | number | px | -500 to 500 | `6.0` | How far the slid field moves sideways, -500 to 500 px, tuned for a picture about 480 px high: positive moves its rows right, negative left, 0 leaves the picture unchanged. A distance between two pixels blends them. Raising it widens the teeth of the comb along every vertical edge. |
| `field` | one of 'odd', 'even' |  |  | `'odd'` | Which field slides: 'odd' the second, fourth, sixth... row (counting from the top as the first), 'even' the first, third, fifth... row. The other field stays where it was. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'wrap', 'nearest', 'reflect', 'mirror' |  |  | `'nearest'` | What fills the gap the slide opens at the side: 'nearest' holds the row's last pixel there, 'wrap' brings back what left on the other side, 'reflect' mirrors the row (the edge pixel repeated once), 'mirror' mirrors it about the edge pixel. |

</details>

### `jpeg_crush`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `quality` | whole number | count | 1 to 100 | `6` | The JPEG quality, 1 to 100, on the scale JPEG tools use (the Independent JPEG Group's): 100 keeps almost everything, about 75 is a normal photo, about 6 shows 8 px blocks, ringing round edges and colour bleed, 1 is a few flat blocks. Lowering it coarsens the quantisation of every 8 x 8 block, so each hit is also a larger jump in the picture. |
| `hits` | whole number | count | 0 to 5000 | `0` | How many coefficients are damaged after the compression, 0 to 5000, each at a random place in a random 8 x 8 block of the brightness or one of the two colour planes, as a corrupted file shows. 0 leaves the plain crushed picture, and then `seed`, `dc_share` and `magnitude` do nothing. Raising it spreads more damage over the picture. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed for where the hits land and how large they are: the same seed always gives the same damage, any other a different one. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `dc_share` | number | fraction | 0 to 1 | `0.3` | The share of hits, 0 to 1, that land on a block's DC value (its average) rather than on one of its 63 detail coefficients. A DC hit shifts the block and, as JPEG stores each average as a difference from the block before, every later block of its row: a streak to the right edge. A detail hit only marks its own block with a pattern. |
| `magnitude` | whole number | count | 1 to 100 | `8` | The largest change a hit makes to a coefficient, 1 to 100 quantisation steps: each hit adds or subtracts a whole number of steps between 1 and this. Raising it makes the damage louder. |

</details>

### `ntsc`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `sigma` | number | px | 0 to 100 | `4.0` | How far the colour smears sideways, as the standard deviation in px of a Gaussian along each row, 0 to 100, tuned for a picture about 480 px high: 0 keeps the colour where it is, raising it bleeds colour further past the edges of what it belongs to. Brightness is not smeared. |
| `crawl` | number | fraction | 0 to 1 | `0.3` | How much of the colour signal leaks into brightness as dot crawl, 0 to 1: a fine diagonal chequer on coloured areas, as strong as the colour is. 0 = none, 1 = the full colour signal; grey areas show none. |
| `phase` | number | cycles | 0 to 1 | `0.0` | Where the chequer of the dot crawl is in its cycle, 0 to 1 (1 is the same as 0): moving it sideways through a full cycle makes the dots crawl; half a cycle swaps light dots and dark dots. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `truncate` | number | count | 0.5 to 10 | `4.0` | Where the smear's Gaussian is cut off, in sigmas, 0.5 to 10: the colour spreads over about this many sigmas on either side. Lower is faster and gives a slightly sharper tail. |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What colour is smeared in from beyond the left and right edges: 'reflect' the picture mirrored (edge pixel repeated), 'nearest' its edge pixel repeated, 'mirror' mirrored about the edge pixel, 'wrap' the opposite side. |

</details>

### `rgb_split`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `angle` | number (cyclic) | degrees | 0 to 360 | `30.0` | The direction red moves, in degrees counter-clockwise from pointing right (as on a protractor), 0 to 360: 0 = right, 90 = up, 180 = left, 270 = down. Blue moves the opposite way. |
| `amount` | number | px | 0 to 500 | `8.0` | How far red and blue each move from where they were, in pixels, 0 to 500; they end up twice this far apart. 0 leaves the picture unchanged. A fraction of a pixel blends the two pixels it lies between. Raising it pulls the colour fringes further apart. |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'nearest', 'wrap', 'reflect', 'mirror' |  |  | `'nearest'` | What a moved channel shows where it has left the picture's edge: 'nearest' repeats the channel's edge pixels (a coloured smear along the edge), 'wrap' brings back what left on the other side, 'reflect' mirrors the channel (the edge line repeated once), 'mirror' mirrors it about the edge line. |

</details>

### `scan_roll`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `axis` | one of 'vertical', 'horizontal' |  |  | `'vertical'` | The direction of the roll: 'vertical' slides the picture up or down, as a television whose vertical hold slipped; 'horizontal' slides it left or right, as a horizontal hold would. |
| `offset` | number | fraction | -1 to 1 | `0.2` | How far the picture rolls, as a fraction of its height ('vertical') or width ('horizontal'), -1 to 1: positive moves it down or to the right, negative up or to the left, 0 leaves it where it is. A whole picture (1 or -1) with 'wrap' brings it back to where it was. A distance that ends between two pixels blends the two (linear interpolation along the roll). |

<details><summary>Advanced parameters (1)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `edge` | one of 'wrap', 'nearest', 'reflect', 'mirror' |  |  | `'wrap'` | What fills the gap the roll opens: 'wrap' brings back what left on the other side, a true roll; 'nearest' holds the picture's first or last line there (a smear); 'reflect' mirrors the picture (the edge line repeated once); 'mirror' mirrors it about the edge line. |

</details>

### `signal_noise`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `bands` | whole number | count | 0 to 200 | `3` | How many horizontal bands of static cross the picture, 0 to 200; 0 = none. Bands land at random rows and may overlap. |
| `band_height` | number | fraction | 0 to 1 | `0.04` | The tallest a band of static can be, as a fraction 0 to 1 of the picture's height. Each band is between half this and this tall (never under 1 px); raising it gives thicker bands. |
| `static` | number | fraction | 0 to 1 | `0.7` | How much static a band holds, 0 to 1: the share of each pixel in a band that is replaced by a random grey. 0 leaves the bands invisible, 1 fills them with pure snow. |
| `dropouts` | whole number | count | 0 to 2000 | `12` | How many dropout streaks, 0 to 2000: short horizontal marks where the signal was lost and the tape shows `color`. 0 = none. Raising it covers more of the picture. |
| `color` | colour |  |  | `'#ffffff'` | The colour of a dropout streak: '#rrggbb', '#rgb' or [r, g, b] 0..1. White is the usual look of a tape dropout; black gives dead gaps. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed for where the bands and streaks fall and the grey of the static: the same seed always gives the same damage, any other a different one. |

<details><summary>Advanced parameters (2)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `dropout_length` | number | fraction | 0 to 1 | `0.12` | The longest a dropout streak can be, as a fraction 0 to 1 of the picture's width. Each streak is between half this and this long (never under 1 px). |
| `dropout_height` | whole number | px | 1 to 100 | `2` | How many rows tall a dropout streak is, 1 to 100 px, tuned for a picture about 480 px high. A streak that would cross the bottom edge is cut there. |

</details>

### `vhs`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `tracking` | number | fraction | 0 to 1 | `0.06` | How far a tracking band slides sideways, at most, as a fraction 0 to 1 of the picture's width: each band slides by a random whole number of pixels up to this, left or right. 0 = the bands do not move. |
| `bands` | whole number | count | 0 to 50 | `3` | How many horizontal tracking bands slide, 0 to 50, at random rows; 0 = none. |
| `bleed` | number | px | 0 to 100 | `5.0` | How far colour bleeds to the right of what it belongs to, 0 to 100 px, tuned for a picture about 480 px high: colour lags the brightness by this many pixels and is blurred sideways by a Gaussian of this standard deviation. 0 = no bleed; raising it smears red and blue far past their edges. |
| `noise` | number | fraction | 0 to 1 | `0.03` | The standard deviation of the tape noise added to brightness, 0 to 1 in picture units: 0 = none, 0.03 is visible, 0.1 is heavy. Colour is not touched. |
| `wobble` | number | px | 0 to 50 | `1.5` | How far the rows wander sideways, as the standard deviation in px of a slow random drift from row to row, 0 to 50, tuned for a picture about 480 px high: 0 = straight rows, a few px = the shaky edges of a worn tape. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed for the bands, the noise and the wobble: the same seed always gives the same damage, any other a different one. |

<details><summary>Advanced parameters (5)</summary>

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `band_height` | number | fraction | 0 to 1 | `0.04` | The tallest a tracking band can be, as a fraction 0 to 1 of the picture's height; each band is between half this and this tall (never under 1 px). |
| `noise_size` | number | px | 0.5 to 50 | `1.5` | How wide a speck of tape noise is, 0.5 to 50 px (up to 1 px every pixel is its own speck), tuned for a picture about 480 px high. Raising it gives coarser grain. |
| `wobble_size` | number | px | 0.5 to 500 | `12.0` | How many rows one wander of the wobble spans, 0.5 to 500 px, tuned for a picture about 480 px high: small is a jitter from row to row, large a slow sway. |
| `truncate` | number | count | 0.5 to 10 | `4.0` | Where the blurs of the bleed, the noise and the wobble are cut off, in sigmas, 0.5 to 10. Lower is faster and gives slightly sharper tails. |
| `edge` | one of 'wrap', 'nearest', 'reflect', 'mirror' |  |  | `'wrap'` | What lies beyond the picture's left and right edges for everything that moves or blurs sideways: 'wrap' the opposite side, 'nearest' the edge pixel repeated, 'reflect' the picture mirrored (edge pixel repeated once), 'mirror' mirrored about the edge pixel. For the noise and wobble the rules act on the rows and columns of their own fields. |

</details>
