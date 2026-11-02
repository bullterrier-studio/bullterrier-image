//! The maps and partitions an effect is handed, `Layer` and `Labels`, through the public API alone.

use bullterrier_image::{Error, Labels, Layer};

#[test]
fn a_layer_keeps_its_values_and_size() -> Result<(), Error> {
    let layer = Layer::new(2, 3, vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0])?;
    assert_eq!((layer.height(), layer.width()), (2, 3));
    assert_eq!(layer.values(), [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
    Ok(())
}

#[test]
fn a_layer_of_values_that_are_not_a_map_is_refused_naming_the_first_bad_pixel() {
    // (height, width, values, what the message must say)
    for (height, width, values, said) in [
        (1, 2, vec![0.5; 3], "3 values"),
        (2, 2, vec![0.5; 3], "3 values"),
        (1, 3, vec![0.5, 1.5, 0.5], "row 0, column 1 is 1.5"),
        (2, 2, vec![0.5, 0.5, 0.5, -0.25], "row 1, column 1 is -0.25"),
        (1, 2, vec![f32::NAN, 0.5], "row 0, column 0 is NaN"),
        (1, 2, vec![0.5, f32::INFINITY], "row 0, column 1 is inf"),
    ] {
        let result = Layer::new(height, width, values);
        assert!(
            matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains(said)),
            "{said}: {result:?}"
        );
    }
    // the closed ends are in
    assert!(Layer::new(1, 2, vec![0.0, 1.0]).is_ok());
}

#[test]
fn a_map_or_partition_with_no_row_or_no_column_is_refused_and_one_pixel_is_enough() {
    for (height, width) in [(0, 5), (5, 0), (0, 0)] {
        for result in [
            Layer::new(height, width, Vec::new()).map(|_| ()),
            Labels::new(height, width, Vec::new(), 1).map(|_| ()),
        ] {
            assert!(
                matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains("at least one")),
                "{height} x {width}: {result:?}"
            );
        }
    }
    assert!(Layer::new(1, 1, vec![0.5]).is_ok());
    assert!(Labels::new(1, 1, vec![0], 1).is_ok());
}

#[test]
fn labels_keep_their_pieces_and_size() -> Result<(), Error> {
    // three pieces, the last of which holds no pixel
    let labels = Labels::new(2, 2, vec![0, 1, 1, 0], 3)?;
    assert_eq!((labels.height(), labels.width(), labels.pieces()), (2, 2, 3));
    assert_eq!(labels.values(), [0, 1, 1, 0]);
    Ok(())
}

#[test]
fn labels_that_are_not_a_partition_are_refused_naming_the_first_bad_pixel() {
    for (labels, pieces, said) in [
        (vec![0, 1, 1], 2, "3 labels"),
        (vec![0, 2], 2, "row 0, column 1 is 2"),
        (vec![0, -1], 2, "row 0, column 1 is -1"),
        (vec![0, 0], 0, "row 0, column 0 is 0"),
    ] {
        let result = Labels::new(1, 2, labels, pieces);
        assert!(
            matches!(&result, Err(error @ Error::Inputs(_)) if error.to_string().contains(said)),
            "{said}: {result:?}"
        );
    }
    assert!(Labels::new(1, 2, vec![0, 1], 2).is_ok());
}
