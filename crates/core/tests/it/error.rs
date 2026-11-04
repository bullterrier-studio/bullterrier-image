//! `Error`, `ErrorKind` and `MapKind`, through the public API alone: which exception of the package each error is
//! raised as, and what each says.

use std::error::Error as _;

use bullterrier_image::{Constraint, Error, ErrorKind, MapKind};
use serde_json::json;

/// One error of every variant (the map ones of each kind), with the kind each must be.
fn rows() -> Vec<(Error, ErrorKind)> {
    let bad_json = Error::Params {
        effect: "grade",
        source: <serde_json::Error as serde::de::Error>::custom("a number is needed"),
    };
    vec![
        (
            Error::Invalid {
                place: None,
                effect: "grade".to_owned(),
                field: "exposure".to_owned(),
                constraint: Constraint::Number,
                value: Box::new(json!("a")),
            },
            ErrorKind::Recipe,
        ),
        (Error::Recipe("step 1: no effect".to_owned()), ErrorKind::Recipe),
        (Error::UnknownEffect("nope".to_owned()), ErrorKind::UnknownEffect),
        (
            Error::map_not_found("subject", "no subject"),
            ErrorKind::SubjectNotFound,
        ),
        (Error::map_not_found("depth", "no depth"), ErrorKind::DepthNotFound),
        (Error::map_not_found("sky", "no sky"), ErrorKind::MapNotFound),
        (Error::InvalidImage("not (h, w, 3)".to_owned()), ErrorKind::InvalidImage),
        (
            Error::Size {
                height: 1,
                width: 2,
                values: 5,
            },
            ErrorKind::InvalidImage,
        ),
        (Error::NonFinite("pixel (0, 0)".to_owned()), ErrorKind::NonFiniteImage),
        (
            Error::ImageRead("cannot read a.png as an image: damaged".to_owned()),
            ErrorKind::ImageRead,
        ),
        (
            Error::ImageWrite("cannot write a.png: no such folder".to_owned()),
            ErrorKind::ImageWrite,
        ),
        (
            Error::BackendUnavailable("cannot write '.bmp' pictures".to_owned()),
            ErrorKind::BackendUnavailable,
        ),
        (
            Error::Lut("cannot read film.cube as a .cube file: not UTF-8".to_owned()),
            ErrorKind::Lut,
        ),
        (Error::Duplicate("grade".to_owned()), ErrorKind::Other),
        (Error::Inputs("no partition".to_owned()), ErrorKind::Other),
        (Error::foreign("fx", "broke"), ErrorKind::Other),
        (bad_json, ErrorKind::Other),
    ]
}

#[test]
fn every_error_is_the_kind_the_package_raises_it_as() {
    for (error, kind) in rows() {
        assert_eq!(error.kind(), kind, "{error}");
    }
}

#[test]
fn the_rows_cover_every_kind_and_all_lists_each_once_with_a_name_of_its_own() {
    let produced: Vec<ErrorKind> = rows().iter().map(|(error, _)| error.kind()).collect();
    for kind in ErrorKind::ALL {
        assert!(produced.contains(kind), "no row makes {kind:?}");
    }
    let mut names: Vec<&str> = ErrorKind::ALL.iter().map(|kind| kind.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), ErrorKind::ALL.len(), "{names:?}");
    assert!(
        names
            .iter()
            .all(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
    );
}

#[test]
fn a_map_error_names_the_subject_and_the_depth_and_every_other_map_is_the_rest() {
    for (map, kind) in [
        ("subject", MapKind::Subject),
        ("depth", MapKind::Depth),
        ("light", MapKind::Other),
        ("Subject", MapKind::Other),
        ("", MapKind::Other),
    ] {
        assert!(
            matches!(Error::map_not_found(map, "m"), Error::MapNotFound { kind: found, map: named, .. } if found == kind && named == map),
            "{map:?}"
        );
    }
}

#[test]
fn a_map_or_image_error_says_its_message_as_it_is() {
    for (error, text) in [
        (
            Error::map_not_found("depth", "the depth map is 2 x 2 but the photo is 3 x 3"),
            "the depth map is 2 x 2 but the photo is 3 x 3",
        ),
        (
            Error::InvalidImage("the picture is 2 x 2 x 4".to_owned()),
            "the picture is 2 x 2 x 4",
        ),
        (
            Error::NonFinite("row 1, column 2 is NaN".to_owned()),
            "row 1, column 2 is NaN",
        ),
    ] {
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn a_foreign_failure_keeps_the_source_it_was_made_from() {
    #[derive(Debug)]
    struct Mine;
    impl std::fmt::Display for Mine {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("mine broke")
        }
    }
    impl std::error::Error for Mine {}

    let error = Error::foreign("fx", Mine);
    assert_eq!(error.to_string(), "the effect \"fx\" failed: mine broke");
    assert!(error.source().is_some_and(<dyn std::error::Error>::is::<Mine>));
    assert!(matches!(&error, Error::Foreign { effect, .. } if effect == "fx"));
    // the source comes back whole, as the bindings need it
    let kept = if let Error::Foreign { source, .. } = error {
        source.downcast::<Mine>().is_ok()
    } else {
        false
    };
    assert!(kept);
}
