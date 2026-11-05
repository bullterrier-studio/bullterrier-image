//! `Lut` built in code, and looked up: the expectations come from the maths of a table, never from the lookup's own
//! formula. An affine map sampled at the grid is reproduced between the nodes (trilinear interpolation is exact on it),
//! a table returns its entries at its nodes, a colour outside the domain takes the edge entry, and a table that is no
//! table is refused.

use bullterrier_image::{Error, ErrorKind, Lut};
use proptest::prelude::*;

use crate::common::compare::assert_close;

const TOLERANCE: f32 = 1e-4;

/// `index` of `top` steps, as a share 0..1.
fn share(index: usize, top: usize) -> f32 {
    f32::from(u16::try_from(index).unwrap_or(u16::MAX)) / f32::from(u16::try_from(top).unwrap_or(u16::MAX))
}

/// The table of `size` entries per axis over `low..high` that holds `map` of the input at each grid point.
fn sampled(size: usize, low: [f32; 3], high: [f32; 3], map: impl Fn([f32; 3]) -> [f32; 3]) -> Result<Lut, Error> {
    let at = |axis: usize, index: usize| low[axis] + (high[axis] - low[axis]) * share(index, size - 1);
    let entries = (0..size * size * size)
        .map(|entry| {
            map([
                at(0, entry % size),
                at(1, entry / size % size),
                at(2, entry / (size * size)),
            ])
        })
        .collect();
    Lut::new(size, low, high, entries)
}

/// An affine map `x -> matrix x + shift`, written out channel by channel.
fn affine(matrix: [[f32; 3]; 3], shift: [f32; 3]) -> impl Fn([f32; 3]) -> [f32; 3] {
    move |x| [0, 1, 2].map(|row| matrix[row][0] * x[0] + matrix[row][1] * x[1] + matrix[row][2] * x[2] + shift[row])
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn an_identity_table_returns_every_colour_as_it_is(size in 2_usize..=33, color in prop::array::uniform3(0.0_f32..=1.0)) {
        let identity = Lut::identity(size)?;
        assert_close(&identity.lookup(color), &color, TOLERANCE, &format!("identity of {size}"));
    }

    #[test]
    fn a_table_sampling_an_affine_map_reproduces_it_between_its_nodes(
        size in 2_usize..=17,
        matrix in prop::array::uniform3(prop::array::uniform3(-2.0_f32..=2.0)),
        shift in prop::array::uniform3(-1.0_f32..=1.0),
        color in prop::array::uniform3(0.0_f32..=1.0),
    ) {
        let map = affine(matrix, shift);
        let table = sampled(size, [0.0; 3], [1.0; 3], &map)?;
        assert_close(&table.lookup(color), &map(color), TOLERANCE, &format!("{size} entries per axis"));
    }

    #[test]
    fn the_domain_only_changes_where_the_nodes_are(
        size in 2_usize..=9,
        low in prop::array::uniform3(-1.0_f32..=0.0),
        span in prop::array::uniform3(0.5_f32..=2.0),
        place in prop::array::uniform3(0.0_f32..=1.0),
    ) {
        // an affine table over the domain `low..low + span`, looked up at a point inside it
        let high = [0, 1, 2].map(|axis| low[axis] + span[axis]);
        let color = [0, 1, 2].map(|axis| low[axis] + span[axis] * place[axis]);
        let map = affine([[0.5, -0.25, 0.0], [0.0, 1.0, 0.5], [-1.0, 0.0, 0.75]], [0.1, 0.2, 0.3]);
        let table = sampled(size, low, high, &map)?;
        assert_close(&table.lookup(color), &map(color), TOLERANCE, &format!("domain {low:?} .. {high:?}"));
    }
}

#[test]
fn a_table_returns_its_entries_at_its_nodes() -> Result<(), Error> {
    // by hand: every node holds a colour of its own, so a wrong index shows
    let size = 4;
    let table = sampled(size, [0.0; 3], [1.0; 3], |x| {
        [x[0] * x[1], x[2] * x[2], 1.0 - x[0] * x[2]]
    })?;
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                let node = [share(red, 3), share(green, 3), share(blue, 3)];
                let expected = [node[0] * node[1], node[2] * node[2], 1.0 - node[0] * node[2]];
                assert_close(
                    &table.lookup(node),
                    &expected,
                    TOLERANCE,
                    &format!("node {red} {green} {blue}"),
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_colour_between_the_entries_is_a_trilinear_mix_of_the_eight_around_it() -> Result<(), Error> {
    // two entries per axis sample a function that is linear in each channel on its own: red x green, blue x (1 - red) and the
    // mean; by hand at (0.25, 0.5, 0.75): 0.125, 0.75 x 0.75 = 0.5625, (0.25 + 0.5 + 0.75) / 3 = 0.5
    let table = sampled(2, [0.0; 3], [1.0; 3], |x| {
        [x[0] * x[1], x[2] * (1.0 - x[0]), (x[0] + x[1] + x[2]) / 3.0]
    })?;
    assert_close(
        &table.lookup([0.25, 0.5, 0.75]),
        &[0.125, 0.5625, 0.5],
        TOLERANCE,
        "multilinear",
    );
    Ok(())
}

#[test]
fn between_two_entries_of_a_curve_the_result_is_their_mean() -> Result<(), Error> {
    // three entries per axis sample the square at 0, 0.5 and 1 (0, 0.25, 1): halfway between the first two is 0.125
    let table = sampled(3, [0.0; 3], [1.0; 3], |x| x.map(|value| value * value))?;
    assert_close(
        &table.lookup([0.25; 3]),
        &[0.125; 3],
        TOLERANCE,
        "the mean of 0 and 0.25",
    );
    Ok(())
}

#[test]
fn entries_are_listed_red_fastest_then_green_then_blue() -> Result<(), Error> {
    let rows: Vec<[f32; 3]> = (0..8_u8).map(|row| [f32::from(row), 10.0, 20.0]).collect();
    let table = Lut::new(2, [0.0; 3], [1.0; 3], rows)?;
    // entry 1 is one step in red, 2 one step in green, 4 one step in blue
    for (corner, row) in [
        ([1.0, 0.0, 0.0], 1.0),
        ([0.0, 1.0, 0.0], 2.0),
        ([0.0, 0.0, 1.0], 4.0),
        ([1.0, 1.0, 1.0], 7.0),
    ] {
        assert_close(
            &table.lookup(corner),
            &[row, 10.0, 20.0],
            TOLERANCE,
            &format!("{corner:?}"),
        );
    }
    Ok(())
}

#[test]
fn colours_beyond_the_domain_take_the_edge_of_the_table() -> Result<(), Error> {
    // over the domain 0.25..0.75 red entries run 0.5..1 and green ones 0..0.5; blue is an identity
    let entries = (0..8_usize)
        .map(|entry| {
            let (red, green, blue) = (entry % 2, entry / 2 % 2, entry / 4);
            [0.5 + share(red, 2), share(green, 2), share(blue, 1)]
        })
        .collect();
    let table = Lut::new(2, [0.25; 3], [0.75; 3], entries)?;
    // red 0 lies below the domain (the first entry, 0.5), green 1 above it (the last, 0.5), blue 0.375 a quarter along
    assert_close(&table.lookup([0.0, 1.0, 0.375]), &[0.5, 0.5, 0.25], TOLERANCE, "edges");
    assert_close(
        &table.lookup([-9.0, 9.0, 0.375]),
        &[0.5, 0.5, 0.25],
        TOLERANCE,
        "far beyond the edges",
    );
    Ok(())
}

#[test]
fn a_table_that_leaves_the_range_is_looked_up_as_it_is() -> Result<(), Error> {
    let table = Lut::new(2, [0.0; 3], [1.0; 3], vec![[1.4, -0.2, 0.5]; 8])?;
    assert_close(
        &table.lookup([0.3, 0.6, 0.9]),
        &[1.4, -0.2, 0.5],
        TOLERANCE,
        "the clamp is the effect's, not the table's",
    );
    Ok(())
}

#[test]
fn a_table_reports_what_it_was_made_of() -> Result<(), Error> {
    let table = Lut::new(2, [-1.0, 0.0, 0.5], [1.0, 2.0, 1.5], vec![[0.0; 3]; 8])?;
    assert_eq!(table.size(), 2);
    assert_eq!(
        (table.domain_min(), table.domain_max()),
        ([-1.0, 0.0, 0.5], [1.0, 2.0, 1.5])
    );
    assert_eq!(table.entries().len(), 8);
    Ok(())
}

#[test]
fn debug_shows_the_shape_and_not_the_entries() -> Result<(), Error> {
    let shown = format!("{:?}", Lut::identity(64)?);
    assert!(shown.len() < 200, "{} characters", shown.len());
    assert!(shown.contains("64"), "{shown}");
    Ok(())
}

/// Entries of the right count, for a table of `size` entries per axis.
fn entries(size: usize) -> Vec<[f32; 3]> {
    vec![[0.5; 3]; size * size * size]
}

#[test]
fn a_table_that_is_none_is_refused_naming_the_rule() {
    let nan = f32::NAN;
    let mut spoilt = entries(2);
    spoilt[3][1] = f32::INFINITY;
    let rows: Vec<(&str, Result<Lut, Error>, &str)> = vec![
        ("one entry per axis", Lut::new(1, [0.0; 3], [1.0; 3], entries(1)), "1"),
        (
            "past the largest size",
            Lut::new(257, [0.0; 3], [1.0; 3], Vec::new()),
            "257",
        ),
        (
            "too few entries",
            Lut::new(2, [0.0; 3], [1.0; 3], entries(2)[..7].to_vec()),
            "has 8 entries, got 7",
        ),
        ("an infinite entry", Lut::new(2, [0.0; 3], [1.0; 3], spoilt), "finite"),
        (
            "a domain with NaN",
            Lut::new(2, [0.0, nan, 0.0], [1.0; 3], entries(2)),
            "finite",
        ),
        (
            "a domain that is empty",
            Lut::new(2, [0.0; 3], [1.0, 0.0, 1.0], entries(2)),
            "below",
        ),
        (
            "a domain upside down",
            Lut::new(2, [1.0; 3], [0.0; 3], entries(2)),
            "below",
        ),
        ("no identity of size 0", Lut::identity(0), "0"),
        ("no identity of size 257", Lut::identity(257), "257"),
    ];
    for (rule, made, named) in rows {
        let error = made.err();
        assert!(
            error
                .as_ref()
                .is_some_and(|error| error.kind() == ErrorKind::Lut && error.to_string().contains(named)),
            "{rule}: {error:?}"
        );
    }
}

#[test]
fn the_smallest_and_the_largest_size_are_tables() -> Result<(), Error> {
    assert_eq!(Lut::identity(Lut::MIN_SIZE)?.size(), 2);
    let largest = Lut::new(Lut::MAX_SIZE, [0.0; 3], [1.0; 3], entries(Lut::MAX_SIZE))?;
    assert_close(
        &largest.lookup([0.3, 0.6, 0.9]),
        &[0.5; 3],
        TOLERANCE,
        "256 entries per axis",
    );
    Ok(())
}
