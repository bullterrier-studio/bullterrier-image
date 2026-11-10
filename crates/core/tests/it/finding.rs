//! Finding a map with a model, through the public API: what goes into the model, what a map comes out of its answer. The
//! model itself is the one external boundary, so a stand-in `Infer` answers; the real models run in `tests/it/finding_models.rs`.

use std::sync::Mutex;

use bullterrier_image::finding::{Infer, ModelFinder, ModelKind, Tensor, models};
use bullterrier_image::{Error, Finder, MapKind, Picture};

use crate::common::Outcome;

type Answer = Box<dyn Fn(&Tensor) -> Result<Tensor, Error> + Send + Sync>;

/// A model stand-in that keeps what it was given and answers with `answer(input)`.
struct Stand {
    seen: Mutex<Vec<Tensor>>,
    answer: Answer,
}

impl Stand {
    fn new(answer: impl Fn(&Tensor) -> Result<Tensor, Error> + Send + Sync + 'static) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            answer: Box::new(answer),
        }
    }
}

impl Infer for &'static Stand {
    fn infer(&self, input: &Tensor) -> Result<Tensor, Error> {
        self.seen
            .lock()
            .map_err(|_| Error::BackendUnavailable("poisoned".into()))?
            .push(input.clone());
        (self.answer)(input)
    }
}

fn leaked(stand: Stand) -> &'static Stand {
    Box::leak(Box::new(stand))
}

fn grey(height: usize, width: usize, value: f32) -> Vec<f32> {
    vec![value; height * width * 3]
}

#[test]
fn the_catalogue_names_each_model_once_with_its_checksum_and_size() {
    let all = models();
    assert!(all.len() >= 2);
    for (index, model) in all.iter().enumerate() {
        assert_eq!(model.sha256.len(), 64, "{}", model.name);
        assert!(model.sha256.chars().all(|c| c.is_ascii_hexdigit()), "{}", model.name);
        assert!(model.url.starts_with("https://"), "{}", model.name);
        assert!(model.size > 1_000_000, "{}", model.name);
        assert!(
            all[index + 1..].iter().all(|other| other.name != model.name),
            "{} twice",
            model.name
        );
    }
}

#[test]
fn a_subject_model_is_given_the_photo_as_a_1024_square_scaled_to_minus_half_to_half() -> Outcome {
    let stand = leaked(Stand::new(|_| {
        Tensor::new(vec![1, 1, 1024, 1024], vec![1.0; 1024 * 1024])
    }));
    let finder = ModelFinder::new(ModelKind::Subject, stand);
    let values = grey(30, 50, 0.75);
    finder.find(&Picture::new(30, 50, &values)?)?;
    let seen = stand.seen.lock().map_err(|_| "poisoned")?;
    let input = &seen[0];
    assert_eq!(input.shape(), [1, 3, 1024, 1024]);
    // a flat picture stays flat when resized: every value is 0.75 - 0.5, in each channel
    assert!(input.values().iter().all(|value| (value - 0.25).abs() < 1e-5));
    Ok(())
}

#[test]
fn a_depth_model_is_given_the_photo_with_its_short_side_518_and_both_sides_multiples_of_14() -> Outcome {
    let stand = leaked(Stand::new(|input| {
        let (height, width) = (input.shape()[2], input.shape()[3]);
        Tensor::new(vec![1, height, width], vec![0.5; height * width])
    }));
    let finder = ModelFinder::new(ModelKind::Depth, stand);
    let values = grey(100, 150, 0.5);
    finder.find(&Picture::new(100, 150, &values)?)?;
    let seen = stand.seen.lock().map_err(|_| "poisoned")?;
    // 150 x 518 / 100 = 777 -> the nearest multiple of 14 is 784 (56 x 14); the short side stays 518 (37 x 14)
    assert_eq!(seen[0].shape(), [1, 3, 518, 784]);
    // ImageNet's mean and std per channel on a flat 0.5: (0.5 - mean) / std
    let plane = 518 * 784;
    for (channel, (mean, std)) in [(0.485, 0.229), (0.456, 0.224), (0.406, 0.225)].into_iter().enumerate() {
        let expected = (0.5 - mean) / std;
        assert!(
            (seen[0].values()[channel * plane] - expected).abs() < 1e-4,
            "channel {channel}"
        );
    }
    Ok(())
}

#[test]
fn a_depth_input_stays_within_the_budget_however_long_the_picture_is() -> Outcome {
    // (height, width): the short side stays 518 while the long side fits; past that both scale down to fit
    for (height, width, wanted) in [
        (736, 3210, [252, 1092]), // a banner: a 518 x 2254 input needs 2.7 GB, so the long side is held to 1092
        (3210, 736, [1092, 252]),
        (100, 700, [154, 1092]),
        (2000, 2000, [518, 518]),
        (480, 768, [518, 826]), // within the budget: as before
        (1, 5000, [14, 1092]),  // an extreme strip still has a whole patch
    ] {
        let stand = leaked(Stand::new(|input| {
            let (rows, columns) = (input.shape()[2], input.shape()[3]);
            Tensor::new(vec![1, rows, columns], vec![0.5; rows * columns])
        }));
        let values = grey(height, width, 0.5);
        ModelFinder::new(ModelKind::Depth, stand).find(&Picture::new(height, width, &values)?)?;
        let seen = stand.seen.lock().map_err(|_| "poisoned")?;
        let [rows, columns] = [seen[0].shape()[2], seen[0].shape()[3]];
        assert_eq!([rows, columns], wanted, "{height} x {width}");
        assert!(
            rows % 14 == 0 && columns % 14 == 0 && rows.max(columns) <= 1092,
            "{rows} x {columns}"
        );
    }
    Ok(())
}

#[test]
fn a_subjects_probability_comes_back_as_a_map_of_the_photos_size_with_the_edge_cleaned() -> Outcome {
    // the left half of the 1024 square is the subject
    let stand = leaked(Stand::new(|_| {
        let values = (0..1024 * 1024)
            .map(|index| if index % 1024 < 512 { 1.0 } else { 0.0 })
            .collect();
        Tensor::new(vec![1, 1, 1024, 1024], values)
    }));
    let finder = ModelFinder::new(ModelKind::Subject, stand);
    let values = grey(40, 60, 0.5);
    let map = finder.find(&Picture::new(40, 60, &values)?)?;
    assert_eq!((map.height(), map.width()), (40, 60));
    let at = |row: usize, column: usize| map.values()[row * 60 + column];
    assert!(at(20, 5) > 0.9, "{}", at(20, 5));
    assert!(at(20, 55) < 0.1, "{}", at(20, 55));
    Ok(())
}

#[test]
fn a_photo_with_nothing_standing_out_is_no_subject_found() -> Outcome {
    let stand = leaked(Stand::new(|_| {
        Tensor::new(vec![1, 1, 1024, 1024], vec![0.2; 1024 * 1024])
    }));
    let values = grey(40, 60, 0.5);
    let refused = ModelFinder::new(ModelKind::Subject, stand).find(&Picture::new(40, 60, &values)?);
    assert!(
        matches!(
            refused,
            Err(Error::MapNotFound {
                kind: MapKind::Subject,
                ..
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn depth_is_scaled_so_the_nearest_pixel_is_1_and_the_farthest_0() -> Outcome {
    // nearness grows to the right: 0 to 10 across the columns
    let stand = leaked(Stand::new(|input| {
        let (height, width) = (input.shape()[2], input.shape()[3]);
        let values = (0..height * width)
            .map(|index| {
                10.0 * f32::from(u16::try_from(index % width).unwrap_or(0))
                    / f32::from(u16::try_from(width - 1).unwrap_or(1))
            })
            .collect();
        Tensor::new(vec![1, height, width], values)
    }));
    let values = grey(100, 150, 0.5);
    let map = ModelFinder::new(ModelKind::Depth, stand).find(&Picture::new(100, 150, &values)?)?;
    assert_eq!((map.height(), map.width()), (100, 150));
    let (low, high) = map
        .values()
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    assert!(low.abs() < 1e-6 && (high - 1.0).abs() < 1e-6, "{low} {high}");
    assert!(map.values()[75 * 150 + 10] < map.values()[75 * 150 + 140]);
    Ok(())
}

#[test]
fn a_flat_depth_answer_is_an_all_zero_map() -> Outcome {
    let stand = leaked(Stand::new(|input| {
        let (height, width) = (input.shape()[2], input.shape()[3]);
        Tensor::new(vec![1, height, width], vec![3.0; height * width])
    }));
    let values = grey(100, 150, 0.5);
    let map = ModelFinder::new(ModelKind::Depth, stand).find(&Picture::new(100, 150, &values)?)?;
    assert!(map.values().iter().all(|&value| value == 0.0));
    Ok(())
}

#[test]
fn an_answer_that_is_not_a_number_is_refused_not_taken_for_a_flat_one() -> Outcome {
    for answer in [f32::NAN, f32::INFINITY] {
        let stand = leaked(Stand::new(move |input| {
            let (rows, columns) = (input.shape()[2], input.shape()[3]);
            Tensor::new(vec![1, rows, columns], vec![answer; rows * columns])
        }));
        let values = grey(100, 150, 0.5);
        let refused = ModelFinder::new(ModelKind::Depth, stand).find(&Picture::new(100, 150, &values)?);
        assert!(
            matches!(&refused, Err(Error::Inputs(message)) if message.contains("map")),
            "{answer}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn an_answer_of_the_wrong_size_is_refused_naming_the_sizes() -> Outcome {
    let stand = leaked(Stand::new(|_| Tensor::new(vec![1, 1, 8, 8], vec![1.0; 64])));
    let values = grey(40, 60, 0.5);
    let refused = ModelFinder::new(ModelKind::Subject, stand).find(&Picture::new(40, 60, &values)?);
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("64") && message.contains("1048576")),
        "{refused:?}"
    );
    Ok(())
}
