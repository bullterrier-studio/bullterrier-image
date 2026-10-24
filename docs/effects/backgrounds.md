# Backgrounds

Generated or replaced backgrounds behind the subject.

[All effects](README.md)

| Example | Effect | What it does | Parameters | Recipe |
| --- | --- | --- | --- | --- |
| <img src="../gallery/gradient.jpg" width="160" alt="gradient"> | `gradient` | Replace the picture with a gradient through `colors`: a straight band, rings round a centre or a sweep. | `kind='linear'` `colors=('#12002b', '#ff2bd6')` `angle=90.0` `x=0.5` `y=0.5` `radius=0.75` `repeat=1` `reflect=False` | [gradient.toml](../../examples/effects/gradient.toml) |

### `gradient`

| Parameter | Type | Unit | Range | Default | Description |
| --- | --- | --- | --- | --- | --- |
| `kind` | one of 'linear', 'radial', 'conic' |  |  | `'linear'` | The layout: 'linear' (colours run in a straight band across the whole picture, at `angle`), 'radial' (rings round the centre `x`, `y`, the last colour `radius` away) or 'conic' (a clockwise sweep round the centre, starting at `angle`; the last colour meets the first at a hard seam). |
| `colors` | list of colours |  |  | `('#12002b', '#ff2bd6')` | The gradient's colours in order, evenly spaced; at least two. Linear: from the start of the band to its end. Radial: from the centre outwards. Conic: round the turn. |
| `angle` | number | degrees | 0 to 360 | `90.0` | Linear: the direction the colours run, measured clockwise on screen from the right: 0 = left to right, 90 = top to bottom. Conic: where the sweep starts, on the same scale (0 = right of the centre, 90 = below it). Ignored by 'radial'. |
| `x` | number | fraction | 0 to 1 | `0.5` | Horizontal position of the centre as a fraction of the width (0 = left edge, 1 = right edge). Used by 'radial' and 'conic'; 'linear' is always centred. |
| `y` | number | fraction | 0 to 1 | `0.5` | Vertical position of the centre as a fraction of the height (0 = top edge, 1 = bottom edge). Used by 'radial' and 'conic'; 'linear' is always centred. |
| `radius` | number | fraction | 0.01 to 4 | `0.75` | Radial only: the distance from the centre at which the last colour is reached, as a fraction of the picture's shorter side; farther out stays on the last colour. Larger = a wider, softer glow. Ignored by 'linear' and 'conic'. |
| `repeat` | whole number | count | 1 to 64 | `1` | How many times the list of colours is fitted along the gradient (the band, the radius, one turn): 1 = once, 2 = twice as fast. Radial stays on the last colour beyond the radius. |
| `reflect` | true or false |  |  | `False` | Run every second repeat backwards, so neighbouring repeats meet on the same colour instead of jumping from the last colour back to the first. Has no effect with `repeat` 1. |
