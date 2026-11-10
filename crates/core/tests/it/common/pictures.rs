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

/// A grey picture of `base` with a pixel `height` lighter in the middle of a `size` x `size` one.
pub(crate) fn bump(size: usize, base: f32, height: f32) -> Vec<f32> {
    let mut grey = vec![base; size * size * 3];
    let centre = (size / 2 * size + size / 2) * 3;
    grey[centre..centre + 3].iter_mut().for_each(|value| *value += height);
    grey
}

/// A vertical step edge in the middle: `low` on the left half, `high` on the right.
pub(crate) fn step(height: usize, width: usize, low: f32, high: f32) -> Vec<f32> {
    (0..height * width * 3)
        .map(|index| if index / 3 % width < width / 2 { low } else { high })
        .collect()
}

/// The values of a 4 x 6 picture counted up through its channels: value `i` is `i / 80`, a different one in every
/// channel (up to 71 / 80).
pub(crate) fn counted() -> Vec<f32> {
    (0_u8..72).map(|index| f32::from(index) / 80.0).collect()
}

/// A picture that says where each pixel was read from: red is the row over the last row, green the column over the last
/// column, blue 0.5 (both sides at least 2). Through a warp that reads between pixels linearly or by a cubic, and stays
/// inside the picture, pixel `(row, column)` of the result holds the place [`places`] gives back.
#[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
pub(crate) fn coordinates(height: usize, width: usize) -> Vec<f32> {
    let (last_row, last_column) = ((height - 1) as f32, (width - 1) as f32);
    (0..height * width)
        .flat_map(|at| [(at / width) as f32 / last_row, (at % width) as f32 / last_column, 0.5])
        .collect()
}

/// The (row, column) place each pixel of `picture` (made from [`coordinates`] by a warp) was read from.
pub(crate) fn places(picture: &[f32], height: usize, width: usize) -> Vec<(f64, f64)> {
    #[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
    let (last_row, last_column) = ((height - 1) as f64, (width - 1) as f64);
    picture
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| (f64::from(pixel[0]) * last_row, f64::from(pixel[1]) * last_column))
        .collect()
}

/// A 5 x 7 RGBA picture with a varied colour and an alpha of 0.4 to 1 at every pixel: the second picture an effect that
/// lays one over the photo is offered, of another size and aspect than the photo's.
pub(crate) fn asset() -> bullterrier_image::Image {
    let values: Vec<f32> = (0..5_u8 * 7)
        .flat_map(|at| {
            [
                f32::from(at * 3 % 11) / 10.0,
                f32::from(at * 5 % 13) / 12.0,
                f32::from(at * 7 % 9) / 8.0,
                0.4 + f32::from(at % 4) * 0.2,
            ]
        })
        .collect();
    bullterrier_image::Image::rgba(5, 7, &values).unwrap_or_else(|_| unreachable!("35 pixels of four channels"))
}
