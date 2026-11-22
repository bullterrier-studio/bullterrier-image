//! The error-diffusion kernels: where each sends a pixel's error, with the sources of the weights.

/// Where an error-diffusion kernel sends a pixel's error: (columns right, rows down, weight) for the pixels it reaches,
/// and the weights' divisor. Every tap lies after the pixel in reading order.
pub(crate) struct Kernel {
    pub(super) taps: &'static [(isize, usize, f32)],
    pub(super) divisor: f32,
    /// For a kernel whose weights depend on the pixel's level: the weights of each tap for levels 0 to 127, replacing the
    /// taps' own weights, with their sum as the divisor.
    pub(super) levels: Option<&'static [[u16; 3]; 128]>, // constant: Ostromoukhov's table has 128 rows, levels 0 to 127
}

/// Floyd and Steinberg (1976): 7/16 right, 3/16 below left, 5/16 below, 1/16 below right.
pub(crate) const FLOYD_STEINBERG: Kernel = Kernel {
    taps: &[(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)], // constant: Floyd and Steinberg (1976)
    levels: None,
    divisor: 16.0, // constant: Floyd and Steinberg (1976)
};

/// Atkinson (Apple MacPaint): 1/8 to each of six pixels, so a quarter of the error is dropped.
pub(crate) const ATKINSON: Kernel = Kernel {
    // constant: Atkinson's published error weights, the taps and the divisor
    taps: &[
        (1, 0, 1.0),
        (2, 0, 1.0),
        (-1, 1, 1.0),
        (0, 1, 1.0),
        (1, 1, 1.0),
        (0, 2, 1.0),
    ], // constant: Atkinson's published error weights
    levels: None,
    divisor: 8.0, // constant: Atkinson's published error weights
};

/// Jarvis, Judice and Ninke (1976): 7/48 and 5/48 right; 3 5 7 5 3 below; 1 3 5 3 1 two below.
pub(crate) const JARVIS_JUDICE_NINKE: Kernel = Kernel {
    // constant: Jarvis, Judice and Ninke (1976), the taps and the divisor
    taps: &[
        (1, 0, 7.0),
        (2, 0, 5.0),
        (-2, 1, 3.0),
        (-1, 1, 5.0),
        (0, 1, 7.0),
        (1, 1, 5.0),
        (2, 1, 3.0),
        (-2, 2, 1.0),
        (-1, 2, 3.0),
        (0, 2, 5.0),
        (1, 2, 3.0),
        (2, 2, 1.0),
    ], // constant: Jarvis, Judice and Ninke (1976)
    levels: None,
    divisor: 48.0, // constant: Jarvis, Judice and Ninke (1976)
};

/// Stucki (1981): JJN's shape with weights 8 4 / 2 4 8 4 2 / 1 2 4 2 1 over 42.
pub(crate) const STUCKI: Kernel = Kernel {
    // constant: Stucki (1981), the taps and the divisor
    taps: &[
        (1, 0, 8.0),
        (2, 0, 4.0),
        (-2, 1, 2.0),
        (-1, 1, 4.0),
        (0, 1, 8.0),
        (1, 1, 4.0),
        (2, 1, 2.0),
        (-2, 2, 1.0),
        (-1, 2, 2.0),
        (0, 2, 4.0),
        (1, 2, 2.0),
        (2, 2, 1.0),
    ], // constant: Stucki (1981)
    levels: None,
    divisor: 42.0, // constant: Stucki (1981)
};

/// Burkes (1988): Stucki without its third row, 8 4 / 2 4 8 4 2 over 32.
pub(crate) const BURKES: Kernel = Kernel {
    // constant: Burkes (1988), the taps and the divisor
    taps: &[
        (1, 0, 8.0),
        (2, 0, 4.0),
        (-2, 1, 2.0),
        (-1, 1, 4.0),
        (0, 1, 8.0),
        (1, 1, 4.0),
        (2, 1, 2.0),
    ], // constant: Burkes (1988)
    levels: None,
    divisor: 32.0, // constant: Burkes (1988)
};

/// Sierra (1989), three rows: 5 3 / 2 4 5 4 2 / 2 3 2 over 32.
pub(crate) const SIERRA: Kernel = Kernel {
    // constant: Sierra (1989), the taps and the divisor
    taps: &[
        (1, 0, 5.0),
        (2, 0, 3.0),
        (-2, 1, 2.0),
        (-1, 1, 4.0),
        (0, 1, 5.0),
        (1, 1, 4.0),
        (2, 1, 2.0),
        (-1, 2, 2.0),
        (0, 2, 3.0),
        (1, 2, 2.0),
    ], // constant: Sierra (1989)
    levels: None,
    divisor: 32.0, // constant: Sierra (1989)
};

/// Sierra (1989), two rows: 4 3 / 1 2 3 2 1 over 16.
pub(crate) const SIERRA_TWO_ROW: Kernel = Kernel {
    // constant: Sierra (1989), two-row, the taps and the divisor
    taps: &[
        (1, 0, 4.0),
        (2, 0, 3.0),
        (-2, 1, 1.0),
        (-1, 1, 2.0),
        (0, 1, 3.0),
        (1, 1, 2.0),
        (2, 1, 1.0),
    ], // constant: Sierra (1989), two-row
    levels: None,
    divisor: 16.0, // constant: Sierra (1989), two-row
};

/// Sierra (1989), Lite: 2 right, 1 below left, 1 below, over 4.
pub(crate) const SIERRA_LITE: Kernel = Kernel {
    taps: &[(1, 0, 2.0), (-1, 1, 1.0), (0, 1, 1.0)], // constant: Sierra (1989), Filter Lite
    levels: None,
    divisor: 4.0, // constant: Sierra (1989), Filter Lite
};

/// Stevenson and Arce (1985), a hexagonal lattice's weights on the square grid: 32 two right; 12 26 30 16 below; 12 26 12; 5 12 12 5, over 200.
pub(crate) const STEVENSON_ARCE: Kernel = Kernel {
    // constant: Stevenson and Arce (1985), the taps and the divisor
    taps: &[
        (2, 0, 32.0),
        (-3, 1, 12.0),
        (-1, 1, 26.0),
        (1, 1, 30.0),
        (3, 1, 16.0),
        (-2, 2, 12.0),
        (0, 2, 26.0),
        (2, 2, 12.0),
        (-3, 3, 5.0),
        (-1, 3, 12.0),
        (1, 3, 12.0),
        (3, 3, 5.0),
    ], // constant: Stevenson and Arce (1985)
    levels: None,
    divisor: 200.0, // constant: Stevenson and Arce (1985)
};

/// Ostromoukhov (2001), variable coefficients: the weights of the right, below-left and below pixels depend on the level of
/// the pixel (its value plus the error passed to it, as lightness in Oklab or the Rec. 709 luma in rgb; clamped to 0 to 1
/// and taken in 256 levels), from the paper's appendix table for levels 0 to 127, mirrored for 128 to 255.
pub(crate) const OSTROMOUKHOV: Kernel = Kernel {
    taps: &[(1, 0, 0.0), (-1, 1, 0.0), (0, 1, 0.0)], // constant: Ostromoukhov (2001), the weights come from `levels`
    divisor: 0.0,                                    // constant: Ostromoukhov (2001), the sum of a level's weights
    levels: Some(&OSTROMOUKHOV_LEVELS),
};

/// The paper's appendix table: (right, below left, below) for levels 0 to 127.
// constant: Ostromoukhov, "A Simple and Efficient Error-Diffusion Algorithm", SIGGRAPH 2001, Appendix I
// (https://perso.liris.cnrs.fr/victor.ostromoukhov/publications/pdf/SIGGRAPH01_varcoeffED.pdf), agreeing with libpipi's
// pipi/dither/ostromoukhov.c (https://gitea.zoy.org/cacalabs/libpipi)
const OSTROMOUKHOV_LEVELS: [[u16; 3]; 128] = [
    [13, 0, 5],
    [13, 0, 5],
    [21, 0, 10],
    [7, 0, 4],
    [8, 0, 5],
    [47, 3, 28],
    [23, 3, 13],
    [15, 3, 8],
    [22, 6, 11],
    [43, 15, 20],
    [7, 3, 3],
    [501, 224, 211],
    [249, 116, 103],
    [165, 80, 67],
    [123, 62, 49],
    [489, 256, 191],
    [81, 44, 31],
    [483, 272, 181],
    [60, 35, 22],
    [53, 32, 19],
    [237, 148, 83],
    [471, 304, 161],
    [3, 2, 1],
    [481, 314, 185],
    [354, 226, 155],
    [1389, 866, 685],
    [227, 138, 125],
    [267, 158, 163],
    [327, 188, 220],
    [61, 34, 45],
    [627, 338, 505],
    [1227, 638, 1075],
    [20, 10, 19],
    [1937, 1000, 1767],
    [977, 520, 855],
    [657, 360, 551],
    [71, 40, 57],
    [2005, 1160, 1539],
    [337, 200, 247],
    [2039, 1240, 1425],
    [257, 160, 171],
    [691, 440, 437],
    [1045, 680, 627],
    [301, 200, 171],
    [177, 120, 95],
    [2141, 1480, 1083],
    [1079, 760, 513],
    [725, 520, 323],
    [137, 100, 57],
    [2209, 1640, 855],
    [53, 40, 19],
    [2243, 1720, 741],
    [565, 440, 171],
    [759, 600, 209],
    [1147, 920, 285],
    [2311, 1880, 513],
    [97, 80, 19],
    [335, 280, 57],
    [1181, 1000, 171],
    [793, 680, 95],
    [599, 520, 57],
    [2413, 2120, 171],
    [405, 360, 19],
    [2447, 2200, 57],
    [11, 10, 0],
    [158, 151, 3],
    [178, 179, 7],
    [1030, 1091, 63],
    [248, 277, 21],
    [318, 375, 35],
    [458, 571, 63],
    [878, 1159, 147],
    [5, 7, 1],
    [172, 181, 37],
    [97, 76, 22],
    [72, 41, 17],
    [119, 47, 29],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [4, 1, 1],
    [65, 18, 17],
    [95, 29, 26],
    [185, 62, 53],
    [30, 11, 9],
    [35, 14, 11],
    [85, 37, 28],
    [55, 26, 19],
    [80, 41, 29],
    [155, 86, 59],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [5, 3, 2],
    [305, 176, 119],
    [155, 86, 59],
    [105, 56, 39],
    [80, 41, 29],
    [65, 32, 23],
    [55, 26, 19],
    [335, 152, 113],
    [85, 37, 28],
    [115, 48, 37],
    [35, 14, 11],
    [355, 136, 109],
    [30, 11, 9],
    [365, 128, 107],
    [185, 62, 53],
    [25, 8, 7],
    [95, 29, 26],
    [385, 112, 103],
    [65, 18, 17],
    [395, 104, 101],
    [4, 1, 1],
];
