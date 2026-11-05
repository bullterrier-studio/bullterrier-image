//! Pictures the tests feed to an effect: `height` x `width`, row after row, three channels a pixel, 0..1.

/// A different value in nearly every channel of every pixel.
pub(crate) fn varied(height: usize, width: usize) -> Vec<f32> {
    varied_seeded(height, width, 0)
}

/// [`varied`], another picture for each `seed`.
pub(crate) fn varied_seeded(height: usize, width: usize, seed: usize) -> Vec<f32> {
    (0..height * width * 3)
        .map(|index| f32::from(u16::try_from((index * 7919 + index / 13 + seed) % 1000).unwrap_or(0)) / 1000.0)
        .collect()
}

/// A grey picture rising from 0 on its left column to 1 on its right one (`width` at least 2).
pub(crate) fn ramp(height: usize, width: usize) -> Vec<f32> {
    #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
    let across = |column: usize| column as f32 / (width - 1) as f32;
    (0..height)
        .flat_map(|_| (0..width).flat_map(move |column| [across(column); 3]))
        .collect()
}

/// A black picture with `pixel` at (`row`, `column`).
pub(crate) fn spot(height: usize, width: usize, row: usize, column: usize, pixel: [f32; 3]) -> Vec<f32> {
    let mut dark = vec![0.0; height * width * 3];
    let at = (row * width + column) * 3;
    dark[at..at + 3].copy_from_slice(&pixel);
    dark
}

/// A black picture with a white pixel at (`row`, `column`).
pub(crate) fn impulse(height: usize, width: usize, row: usize, column: usize) -> Vec<f32> {
    spot(height, width, row, column, [1.0; 3])
}
