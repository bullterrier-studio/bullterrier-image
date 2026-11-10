# Parameter declarations

How an effect's parameters are declared in Rust and seen from Python. The rules that bind are in `CLAUDE.md` ("Parameters");
read this before adding or changing a parameter or the derive.

An effect the core runs **declares its fields in Rust**, on its public `Params` struct (`#[derive(Debug, Clone, PartialEq, Deserialize, Params)]`, from
`crate::params`): each field a type, a `///` doc comment and `#[param(default = .., unit = px, min = .., max = ..,
at_most = other, drivable, advanced, cyclic)]` (`at_most = other`: the number may not exceed the number field `other`, of the
same unit, as `band_min_height` stays below `band_max_height`; `Table::check` and the Python model both apply it); a choice is a fieldless `#[derive(Choice)]` enum (`default = Variant`). The struct's own
doc is the model's docstring, a field's doc its help text in Python: plain text, no rustdoc links, a blank `///` line
becomes a double space. The build refuses a broken declaration, naming the field (an unknown unit is rustc's own
error; that a choice's doc names every variant is checked when Python builds the model). The Python class is
`class XParams(EffectParams, core="x")` with an empty body (`# noqa: D101`); the model is built from the Rust
declaration, a default or bound keeping the literal as written (a whole number stays an `int`). Python's model validates a
user's values when a step is made; `Table::check` in Rust is the same rules, compared with Python's by parity tests
(`tests/_native`); running an effect trusts its parameters. Rust kinds: `f32`, `i32`, `u32` (a seed: 0..4294967295), `bool`, `Color`, `Vec<Color>`
(`#[param(default = ["#12002b", "#ff2bd6"], min_length = 2, max_length = 8)]`: `min_length` of at least 1 required, `max_length`
optional, the default lowercase `#rrggbb`; Python sees a `tuple[Color, ...]`), `Option<Color>` (`#[param()]`, no default: none until
a step gives one, and the `///` doc says what none means; written `nullable`; Python sees `Color | None`), `FieldName`
(`#[param(default = "depth")]`: the name of the selection the effect works from, which it finds as a `Layer` in `Inputs`
under the parameter's name, and `Native` lists as one of its `fields()`; Python sees a `FieldName`), `LutName` (`#[param()]`, no
default: none is the identity table; the table reaches the effect in `Inputs`, see `docs/architecture.md`; Python sees a `Lut`), `PictureName`
(`#[param()]`, nullable, no default: none is no picture; Python sees `Picture | None`), `String` (text:
`#[param(default = " .:-=+*#%@", min_length = 2, max_length = 128, min_distinct = 2)]`, each rule optional and counting characters,
`min_distinct` the fewest different ones; Python sees `str`) and a
choice; an effect with any other (other optional values) keeps a Python model, each field `param(default, doc=...)`, until its
port extends the macro.

**The format.** What a parameter is, as JSON (keys, kinds, the closed list of units, ranges, `cyclic`, `drivable`,
`advanced`), is the ecosystem's parameter format, version 3: [docs/parameter-format/parameters.md](docs/parameter-format/parameters.md),
with its [schema](docs/parameter-format/schemas/parameter-3.schema.json) and
[fixtures](docs/parameter-format/fixtures/parameters). The copy is not edited: a change is a new version in the
ecosystem's format first, then a new copy here. `tests/_native/test_parameter_format.py` validates every field of every
Rust-declared effect. The Rust declaration covers every kind but a nullable `text`/`field` and a number list; `picture` (`PictureName`)
is a name or path, never a colour, and `usize` counts are kind `int`. What follows is what image adds.
