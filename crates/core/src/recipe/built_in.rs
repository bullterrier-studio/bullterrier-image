//! The maps every photo has under a name, which a recipe need not declare: the one place that says what each is.

/// A built-in map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuiltIn {
    /// The subject's alpha, white = subject.
    Subject,
    /// How near, 1 = nearest.
    Depth,
    /// Everything but the subject.
    Background,
    /// The brightness of the working picture.
    Light,
}

/// Where a built-in map comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// The photo is handed it or finds it.
    Found,
    /// One minus another built-in map.
    Inverse(BuiltIn),
    /// Made from the working picture (framed and sized, before any step).
    Picture,
}

impl BuiltIn {
    /// Every built-in map, in the order messages list them.
    pub(crate) const ALL: [Self; 4] = [Self::Subject, Self::Background, Self::Depth, Self::Light];

    /// The built-in map called `name`, if there is one.
    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|built_in| built_in.name() == name)
    }

    /// The name a recipe writes it by.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Subject => "subject",
            Self::Depth => "depth",
            Self::Background => "background",
            Self::Light => "light",
        }
    }

    /// Where it comes from.
    pub(crate) const fn origin(self) -> Origin {
        match self {
            Self::Subject | Self::Depth => Origin::Found,
            Self::Background => Origin::Inverse(Self::Subject),
            Self::Light => Origin::Picture,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BuiltIn, Origin};

    #[test]
    fn each_built_in_is_found_by_its_own_name_and_nothing_else_is() {
        for built_in in BuiltIn::ALL {
            assert_eq!(BuiltIn::named(built_in.name()), Some(built_in));
        }
        for name in ["", "Subject", "subjects", "star"] {
            assert_eq!(BuiltIn::named(name), None, "{name}");
        }
    }

    #[test]
    fn a_map_is_found_made_from_another_or_made_from_the_picture() {
        let origins: Vec<_> = BuiltIn::ALL.into_iter().map(BuiltIn::origin).collect();
        assert_eq!(
            origins,
            [
                Origin::Found,
                Origin::Inverse(BuiltIn::Subject),
                Origin::Found,
                Origin::Picture
            ]
        );
    }
}
