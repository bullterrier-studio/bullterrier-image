# bullterrier-image-derive

The derive macros (`Params`, `Choice`) with which an effect of
[bullterrier-image](https://github.com/bullterrier-studio/bullterrier-image) declares its parameters once, in Rust: the
documentation, defaults, units and ranges written on the fields become the parameter table that validates a recipe, builds
the documentation and the Python signature.

It is an implementation detail of the `bullterrier-image` crate, which re-exports the derives; depend on that crate, not on
this one.

MIT licensed.
