# Parameter description

Version 2, 2026-11-04. Owner: bullterrier-image. The one format that image and vector emit for a parameter. Video, slider
builders and generated stubs read it without knowing what the parameter belongs to.

It is a data format, not shared code: each package keeps its own types and proves it writes this format.

Version 2 only adds: every valid version 1 parameter is a valid version 2 parameter. Vector keeps writing version 1.

## Where the two packages differed

Checked against the code on 2026-10-31.

| | image | vector |
|---|---|---|
| Explanation | `doc` | `meaning` |
| Unit names | `px fraction degrees stops count ratio cycles density` | `px deg cycle fraction`, `""` for none |
| No unit | `null` | `""` |
| Cyclic | stored flag; only `degrees`, range exactly 360 | derived from the unit: `deg` and `cycle` |
| Cyclic upper end | `[min, max]` kept: 360 stays 360 (picture(min) equals picture(max) is a contract test) | `[min, max)`: `max` is stored as `min` |
| Bounded degrees | 10 fields | none (a `deg` is always cyclic) |
| Kinds in the Rust declaration | float, int, bool, choice, color, colors | numbers only |
| Absent values | every key written; `null`, `[]`, `false` | `kind`, `cyclic`, `choices` and the flags do not exist |
| `min == max` | allowed | refused |
| Reserved names | step keys, `partition`, `size_ratio` | `into`, `pen_mm` |
| Number type | `f32` | `f64` |

Image's Python layer is separate: 72 of 84 effects are Pydantic models whose schema uses `description`, `minimum` and
`maximum`. Only the effects declared in Rust (12 today, growing with each port) emit this format.

## The format

One parameter is a JSON object. Schema: [schemas/parameter-2.schema.json](schemas/parameter-2.schema.json).
Examples: [fixtures/parameters/](fixtures/parameters/) (`valid.json`, `invalid.json`).

| Key | Required | Meaning |
|---|---|---|
| `name` | yes | Letters, digits and `_`, starting with a letter: the key under which a value is given |
| `kind` | yes | `float`, `int`, `bool`, `choice`, `color`, `colors`, `field`, `text`, `picture`, `lut` |
| `meaning` | yes | What it does, in the field's own terms; may be empty |
| `default` | yes | By kind: a number, a switch, a choice's name, `"#rrggbb"`, an array of `"#rrggbb"` (`colors`), a name (`field`), a string (`text`), `"#rrggbb"` (`picture`: a flat picture), `null` (`lut`: the identity table); `null` for any kind with `nullable` |
| `unit` | yes | One of the closed list below for `float` and `int`; `null` for the other kinds |
| `min`, `max` | yes | Numbers for `float` and `int`; `null` otherwise |
| `cyclic` | yes | `true` only for a `float` in `degrees` or `cycles` |
| `choices` | no | The names of a `choice`, in declaration order; absent means none |
| `min_length` | `colors`: yes; `text`: no | Fewest elements of a `colors` (at least 1), fewest characters of a `text`; absent on `text` means 0. Absent on every other kind |
| `max_length` | no | Most elements or characters, or `null`; absent means no limit. Only on `colors` and `text` |
| `nullable` | no | `true`: the value may be left out, and `default` may be `null` (`lut` is always so and does not write it). Absent means false. Not on `bool` or `choice` |
| `drivable` | no | A pixel-by-pixel drive may vary it; absent means false |
| `advanced` | no | Tunes the technique rather than chooses the look; absent means false |

A package supports the kinds it needs (vector: `float` and `int`). A reader refuses a `kind` or `unit` it does not know and
ignores a key it does not know. A number is a JSON number: `2` and `2.0` are the same value, and the precision is the
owner package's.

### Kinds added in version 2

- `colors`: a list of colours (a palette, gradient stops). The list has at least `min_length` elements; there is no
  element bound.
- `field`: the name of a selection (`depth`, `light`, `subject`, or a region the recipe declares). A non-empty string; the
  default is a name.
- `text`: free text, such as the characters of a ramp.
- `picture`: a second picture (a texture, a logo), or a colour for a flat one.
- `lut`: a 3D colour lookup table, from a `.cube` file. `default` is `null`: the identity table, which changes nothing.
- A `nullable` parameter has no value until one is given, and `default` `null` says so. `lut` is the one kind whose
  default is always `null`, so its `nullable` is not written.

### Values

How a value is given for a parameter, as opposed to how its description is written (a default is always written in the
canonical spelling, a value may use any accepted one). A value that breaks its kind's rules is refused, naming the effect,
the parameter, the rule and the value, never clamped.

| Kind | Value |
|---|---|
| `float`, `int` | A JSON number; `int` is whole |
| `bool` | `true` or `false` |
| `choice` | One of `choices` |
| `color` | `"#rrggbb"`, `"#rgb"`, or three numbers `[r, g, b]` in 0..1 |
| `colors` | An array of colour values, each written as for `color`; the spellings may be mixed |
| `field` | A name |
| `text` | A string |
| `picture` | A colour value (a flat picture), or a path string, resolved relative to the recipe file; a string starting with `#` is a colour. In code, a picture object |
| `lut` | A path string to a `.cube` file, resolved relative to the recipe file. In code, a table object |

A `nullable` parameter also takes `null`, or no value, for none.

### Units: a closed list

`px` (design pixels), `fraction` (0..1), `degrees`, `stops`, `count`, `ratio` (1 leaves things as they are), `cycles`
(a phase), `density` (marks per 1000 design px²). Adding one changes this page first.

### Rules

Checked by each package's own declaration (the schema cannot do arithmetic):

1. `min <= max`, and `default` lies within them. A whole-number kind has whole `default`, `min` and `max`.
2. A cyclic `degrees` range is exactly 360 and a cyclic `cycles` range exactly 1. Any real value is accepted and wrapped
   into the turn. Every other number is refused outside `min..=max`, not clamped.
3. A cyclic value at `max` and one at `min` give the same result. Whether `max` reads back as `max` (image) or as `min`
   (vector) is the owner's; a reader must not depend on it.
4. `px` values are design pixels: rendering at another size multiplies them.
5. For `colors` and `text`: `min_length <= max_length` when both are given, and `default` has a length within them. Every
   element of a `colors` default is `"#rrggbb"`. Lengths count elements, and characters for `text`.
6. A `lut` default is `null`; any other kind's default is `null` only when `nullable`.
7. Names a package reserves (vector: `into`, `pen_mm`) are its own rule, not the format's.

## What each package changes

Decided 2026-10-31.

- **`meaning`, not `doc`.** Image writes `meaning` (the docs' word). Vector already does.
- **`null` for no unit.** Vector writes `null` where it writes `""` now.
- **The closed unit list is image's, spelled out.** Vector renames `deg` to `degrees` and `cycle` to `cycles`.
- **`cyclic` is a key.** Image already has it. Vector writes it, derived from the unit for now: a bounded angle in vector
  is not possible yet, and adding one is a vector decision for when a drawing needs it.
- **`kind`.** Vector writes `"float"` (and `"int"` once a Param can be one). Image already writes it.
- **Cyclic `cycles` is allowed by the format, not required of image.** Image refuses it today (only `degrees`); it need not
  change until an effect wants a cyclic phase.
- **Optional keys.** Vector omits `choices`, `drivable` and `advanced`. Image keeps writing them.
- **`min == max`.** Image keeps allowing it. The format says `<=`; vector keeps its stricter rule.
- **Cyclic upper end** stays as each owner has it (rule 3).

## What changed in version 2

Decided 2026-11-04, from the 22 parameters image's Python effects still had that version 1 could not describe: 8 lists of
colours, 4 optional colours, 10 field names, a picture, a lookup table and free text. There are no lists of numbers.

- **New kinds:** `colors`, `field`, `text`, `picture`, `lut`.
- **New keys:** `min_length`, `max_length`, `nullable`. Like `choices`, each is written only where it applies.
- **New section:** "Values", which says how a value is written for each kind.
- **Nothing removed or renamed.** Image's Rust declaration supports `colors` first; the other new kinds arrive with their
  first effect.

## Proof

Each package has one conformance test:

1. Serialise every description the package emits (image: every effect declared in Rust, and a rule that fails when an
   effect is ported without joining the test; vector: `parameters()` of the registered verbs and a few Params) and
   validate each against the schema.
2. Check the fixtures: every `valid.json` parameter is accepted and every `invalid.json` one is refused.

The schema and fixtures are copied into each repository (not imported), with the version in the name. A change to the format
is a new version here first, then a copy in each package.

## Where it is written down today

After the alignment, each of these links here and keeps only what is its own (how it implements the format):

- image: `CLAUDE.md` "Parameters"; `docs/design-decisions.md` (cyclic, drivable, design pixels); `crates/core/src/params.rs`
  module docs.
- vector: `docs/formats.md` (the Param section and its example); `docs/api.md` (Param); `schemas/drawing-1.schema.json`
  (the Param definition: it must equal this schema's).
- this folder: `image.md` (the three conventions), `vector.md`, `README.md`.
