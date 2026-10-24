# Distortion

Warps that bend, ripple or displace the picture's geometry.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/fisheye.jpg" width="160" alt="fisheye"> | `fisheye` | Bend the picture as a fisheye lens with its axis at `x`, `y` would, magnifying the middle. | `amount=0.6` `x=0.5` `y=0.5` `interpolation='bilinear'` | [fisheye.toml](../../examples/effects/fisheye.toml) |
| <img src="../gallery/lens_distortion.jpg" width="160" alt="lens_distortion"> | `lens_distortion` | Bend the picture as a lens with radial distortion `distortion` about the axis `x`, `y` does. | `distortion=0.2` `fit='frame'` `x=0.5` `y=0.5` `interpolation='bilinear'` `edge='reflect'` | [lens_distortion.toml](../../examples/effects/lens_distortion.toml) |
| <img src="../gallery/mirror.jpg" width="160" alt="mirror"> | `mirror` | Make the picture symmetric: keep one half and replace the other by its reflection. | `axis='vertical'` `keep='first'` | [mirror.toml](../../examples/effects/mirror.toml) |
| <img src="../gallery/pinch_bulge.jpg" width="160" alt="pinch_bulge"> | `pinch_bulge` | Push the picture out (`amount` above 0, a bulge) or in (below 0, a pinch) about the centre `x`, `y`. | `amount=0.5` `radius=0.5` `x=0.5` `y=0.5` `interpolation='bilinear'` `edge='reflect'` | [pinch_bulge.toml](../../examples/effects/pinch_bulge.toml) |
| <img src="../gallery/polar.jpg" width="160" alt="polar"> | `polar` | Wrap the picture round the centre `x`, `y` into a planet (`mode` 'to'), or unwrap such a planet ('from'). | `mode='to'` `x=0.5` `y=0.5` `interpolation='bilinear'` `edge='nearest'` | [polar.toml](../../examples/effects/polar.toml) |
| <img src="../gallery/slice_shift.jpg" width="160" alt="slice_shift"> | `slice_shift` | Cut the picture into horizontal strips at random rows and slide each sideways by a random distance. | `slices=30` `amount=0.05` `seed=0` | [slice_shift.toml](../../examples/effects/slice_shift.toml) |
| <img src="../gallery/swirl.jpg" width="160" alt="swirl"> | `swirl` | Twist the picture about the centre `x`, `y` by `angle` degrees there, falling to nothing at `radius`. | `angle=90.0` `radius=0.5` `x=0.5` `y=0.5` `interpolation='bilinear'` `edge='reflect'` | [swirl.toml](../../examples/effects/swirl.toml) |
| <img src="../gallery/tile.jpg" width="160" alt="tile"> | `tile` | Repeat the whole picture, scaled down, in a grid of `columns` by `rows`. | `columns=3` `rows=3` | [tile.toml](../../examples/effects/tile.toml) |

### `fisheye`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `amount` | number | fraction | 0 to 0.9 | `0.6` | How wide the lens sees, as a fraction of 90 degrees: the half angle of its field of view, reaching the farthest corner of the picture, is amount x 90 degrees (0.5 is a 90 degree lens, 0.9 a 162 degree one). Larger bends the picture more: the middle swells and the edges are squeezed in toward the corner; 0 leaves the picture unchanged. Wider than 0.9 is not offered, as the middle would grow beyond all reason. |
| `x` | number | fraction | 0 to 1 | `0.5` | The lens axis's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. |
| `y` | number | fraction | 0 to 1 | `0.5` | The lens axis's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle, 1 = the last. |
| `interpolation` | one of 'nearest', 'bilinear', 'bicubic' |  |  | `'bilinear'` | How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value exactly as it is but the magnified middle turns blocky), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges). |

### `lens_distortion`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `distortion` | number | ratio | -0.3 to 1 | `0.2` | The lens's radial distortion coefficient (the k1 of the Brown-Conrady model), with the distance from the axis measured in units of the farthest corner's. Positive is barrel distortion: straight lines bow outward and the picture looks wrapped around a ball. Negative is pincushion: lines bow inward and the corners are pulled out. The size is how much farther from the axis than its own place each pixel's source lies at the farthest corner: 0.2 is 20% farther. -0.3 is the most pincushion that does not fold the picture over, 1 the strongest barrel; 0 leaves the picture unchanged. |
| `fit` | one of 'frame', 'centre' |  |  | `'frame'` | How the picture is scaled back after the bend: 'frame' keeps the farthest corner in its corner, so the picture fills the frame (a barrel is zoomed in and loses its sides, and nothing beyond the picture is shown; a pincushion is zoomed out toward the middle and pulls in what lies beyond the middle of each edge, which the edge rule fills); 'centre' keeps the scale at the lens axis, so a barrel shrinks toward the middle, and the edge rule fills the corners it exposes. |
| `x` | number | fraction | 0 to 1 | `0.5` | The lens axis's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. |
| `y` | number | fraction | 0 to 1 | `0.5` | The lens axis's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle, 1 = the last. |
| `interpolation` | one of 'nearest', 'bilinear', 'bicubic' |  |  | `'bilinear'` | How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value exactly as it is but lines turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges). |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What shows where the bend reads from beyond the picture (with fit 'centre', or a pincushion): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). |

### `mirror`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `axis` | one of 'vertical', 'horizontal', 'both' |  |  | `'vertical'` | The mirror line, named for the line itself: 'vertical' puts a vertical line down the middle, so the two sides are each other's reflection (left against right, as a face made symmetric); 'horizontal' puts a horizontal line across the middle (top against bottom, as a reflection in water); 'both' does both, so the picture is symmetric in four quarters. |
| `keep` | one of 'first', 'last' |  |  | `'first'` | Which side of each mirror line stays as it is: 'first' keeps the left half (and, for 'horizontal' or 'both', the top half), 'last' keeps the right half (and the bottom half). The other side is replaced by the reflection of the kept one. On a picture with an odd width or height the middle column or row lies on the line and stays, whichever side is kept. |

### `pinch_bulge`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `amount` | number | fraction | -1 to 1 | `0.5` | How far the middle is pushed. Positive is a bulge: the picture at the centre is magnified, as seen through a lens or a bubble, ever more toward the centre and without limit at 1 (the pixels around the centre all show the centre pixel). Negative is a pinch: the picture at the centre is shrunk, as if drawn into a point, down to half size at -1. The push fades smoothly to nothing at the radius, so no pixel is folded over; 0 leaves the picture unchanged. |
| `radius` | number | fraction | 0 to 2 | `0.5` | How far from the centre the effect reaches, as a fraction of the shorter side of the picture, measured from the first pixel to the last (0.5 = a disc as wide as half of it, 2 = a disc that covers the whole picture and more). Pixels from this far out are untouched; 0 leaves the picture unchanged. |
| `x` | number | fraction | 0 to 1 | `0.5` | The centre's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. |
| `y` | number | fraction | 0 to 1 | `0.5` | The centre's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle, 1 = the last. |
| `interpolation` | one of 'nearest', 'bilinear', 'bicubic' |  |  | `'bilinear'` | How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value exactly as it is but a bulge turns blocky), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges). |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What a pinch sees beyond the picture's edge (it reads from farther out than it shows): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). |

### `polar`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `mode` | one of 'to', 'from' |  |  | `'to'` | Which way to convert: 'to' wraps the picture round the centre into a tiny planet (its bottom row at the centre, its top row on the rim of a circle as wide as the shorter side, the left edge straight up and the columns running clockwise, so the sky of a panorama ends up outside); 'from' is the opposite, it takes a picture that is such a planet round the centre and unwraps it into a flat picture, so 'from' undoes 'to'. |
| `x` | number | fraction | 0 to 1 | `0.5` | The planet's centre horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. |
| `y` | number | fraction | 0 to 1 | `0.5` | The planet's centre vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle, 1 = the last. |
| `interpolation` | one of 'nearest', 'bilinear', 'bicubic' |  |  | `'bilinear'` | How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value exactly as it is but the middle of a planet turns blocky), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges). |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'nearest'` | What lies beyond the planet's rim in 'to', or beyond the picture where an off-centre planet leaves it in 'from' (the angle itself never has an edge; it wraps round): 'nearest' (the edge pixel repeated: the sky stays one flat tone of the top row), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'wrap' (the opposite edge). |

### `slice_shift`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `slices` | whole number | count | 1 to 500 | `30` | How many horizontal strips the picture is cut into, 1 to 500 (never more than it has rows). The cuts fall at random rows, so the strips differ in height; 1 slides the whole picture as one. More strips make a finer, busier tear. |
| `amount` | number | fraction | 0 to 1 | `0.05` | The furthest a strip slides, as a fraction 0 to 1 of the picture's width: each strip slides by a random whole number of pixels between that far left and that far right, so a strip may also stay put. 0 leaves the picture unchanged; 1 can slide a strip by a whole picture width. |
| `seed` | whole number | count | 0 to 4294967295 | `0` | Random seed for where the cuts fall and how far each strip slides: the same seed always gives the same tear, any other seed gives a different one. |

### `swirl`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `angle` | number | degrees | -720 to 720 | `90.0` | How far the picture is turned at the centre, in degrees: positive turns clockwise, negative counter-clockwise, 360 is a whole turn. The turn falls off with the square of the distance left to the radius, so the picture is twisted into a spiral and unturned at the radius. Larger is a tighter spiral; 0 leaves the picture unchanged. |
| `radius` | number | fraction | 0 to 2 | `0.5` | How far from the centre the swirl reaches, as a fraction of the shorter side of the picture, measured from the first pixel to the last (0.5 = a disc as wide as half of it, 2 = a disc that covers the whole picture and more). Pixels from this far out are untouched; 0 leaves the picture unchanged. |
| `x` | number | fraction | 0 to 1 | `0.5` | The centre's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. |
| `y` | number | fraction | 0 to 1 | `0.5` | The centre's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle, 1 = the last. |
| `interpolation` | one of 'nearest', 'bilinear', 'bicubic' |  |  | `'bilinear'` | How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges). |
| `edge` | one of 'reflect', 'nearest', 'mirror', 'wrap' |  |  | `'reflect'` | What the swirl sees beyond the picture's edge (a swirl near the border turns pixels in from outside): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). |

### `tile`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `columns` | whole number | count | 1 to 100 | `3` | How many copies of the picture sit side by side, 1 to 100; 1 keeps the picture whole across. Each copy is the width of the picture divided by this, rounded down (at least 1 px). |
| `rows` | whole number | count | 1 to 100 | `3` | How many copies sit one above another, 1 to 100; 1 keeps the picture whole down. Each copy is the height of the picture divided by this, rounded down (at least 1 px). |
