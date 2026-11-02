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

    /// The names of all of them, in the order of [`ALL`](Self::ALL). A list, so that it chains with borrowed names.
    pub(crate) fn names<'a>() -> Vec<&'a str> {
        Self::ALL.map(Self::name).to_vec()
    }

    /// The names of those the photo is handed or finds.
    pub(crate) fn found<'a>() -> Vec<&'a str> {
        Self::ALL
            .into_iter()
            .filter(|built_in| built_in.origin() == Origin::Found)
            .map(Self::name)
            .collect()
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
