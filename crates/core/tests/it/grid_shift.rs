//! A torn grid that tears by nothing is the straight grid, whatever the shares and the size: the shift of the pixel centres
//! the noise makes is `amount` times the noise, and none at all when the amount is 0.

use crate::common::compare::first_difference;
use crate::common::ops::partition;
use bullterrier_image::region::{Edge, EdgeStyle, Grid, Shares};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn a_torn_edge_of_no_amount_gives_the_pieces_of_the_straight_grid(
        height in 1_usize..12,
        width in 1_usize..12,
        columns in proptest::collection::vec(0.1_f64..5.0, 1..5),
        rows in proptest::collection::vec(0.1_f64..5.0, 1..5),
        seed in any::<u32>(),
    ) {
        let grid = |edge| Grid { edge, columns: Shares(columns.clone()), rows: Shares(rows.clone()), ..Grid::default() };
        let torn = Edge { style: EdgeStyle::Torn, amount: 0.0, seed, ..Edge::default() };
        let straight = partition((height, width), None, grid(Edge::default()))?;
        let nothing = partition((height, width), None, grid(torn))?;
        prop_assert_eq!(first_difference(&straight.0, &nothing.0), None);
        prop_assert_eq!(straight.1, nothing.1);
    }
}
