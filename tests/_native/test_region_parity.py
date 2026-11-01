"""The Rust core's regions, `each` steps, `needs` and region stages against what they should be.

Every row has an independent column: a numpy formula written from the definition (a gradient's position along its
direction, the cell a pixel's centre falls in, a Gaussian of the pieces' indicator by `scipy.ndimage`) or a property.
`each` with `assign="random"` draws from the core's own stream, not numpy's: it is tested by properties only.
"""

from collections.abc import Callable
from pathlib import Path
from typing import Any

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import bridge
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.params import Drive
from bullterrier_image.core.regions import (
    Band,
    Bands,
    Combine,
    Cuts,
    Edge,
    Given,
    Grid,
    LinearGradient,
    Pick,
    RadialGradient,
    Shards,
    Subject,
    Svg,
)
from bullterrier_image.core.steps import Each, Recipe, Step, references
from bullterrier_image.errors import DepthNotFoundError, MapNotFoundError, RecipeError, SubjectNotFoundError
from bullterrier_image.loader import load_recipe

ROOT = Path(__file__).resolve().parents[2]
SHIPPED = sorted((ROOT / "src" / "bullterrier_image" / "looks").glob("*.toml"))
EXAMPLES = sorted(path for path in (ROOT / "examples").rglob("*.toml") if path.name != "bt-image.toml")
FILES = [*SHIPPED, *EXAMPLES]

DOUBLED = Step("grade", params={"exposure": 1}, opacity=1)
QUADRUPLED = Step("grade", params={"exposure": 2}, opacity=1)


def depth_of(photo: np.ndarray) -> np.ndarray:
    """Depth rising from 0 at the left to 1 at the right."""
    height, width = photo.shape[:2]
    return np.tile(np.linspace(0, 1, width, dtype=np.float32), (height, 1))


def stops(photo: np.ndarray, count: int) -> np.ndarray:
    """What `grade(exposure=count)` makes of `photo`: every stop doubles every value, up to white."""
    return np.clip(photo * 2.0**count, 0.0, 1.0)


def over(photo: np.ndarray, source: np.ndarray, alpha: np.ndarray) -> np.ndarray:
    """`source` laid over `photo` by `alpha` (the compositing equation for the normal blend mode)."""
    return photo + np.asarray(alpha, np.float64)[..., None] * (source - photo)


def masked_by(name: str, **regions: Any) -> Recipe:
    """A recipe that doubles the photo through the selection `name`, declaring `regions`."""
    return Recipe([Step("grade", params={"exposure": 1}, mask=name)], regions=regions)


def core_render(photo: np.ndarray, recipe: Recipe, **given: Any) -> np.ndarray:
    return bridge.render(photo, recipe, **given)


# --- the definitions, in numpy ------------------------------------------------------------------------------------


def linear(shape: tuple[int, int], degrees: float) -> np.ndarray:
    """A pixel's position along `degrees` (0 = left to right, 90 = top to bottom) over the extent of the picture in that
    direction, centred: 0 at the start of the extent, 1 at the end, measured from the centres of the pixels."""
    height, width = shape
    cosine, sine = np.cos(np.radians(degrees)), np.sin(np.radians(degrees))
    across, down = np.arange(width) + 0.5 - width / 2, np.arange(height) + 0.5 - height / 2
    extent = width * abs(cosine) + height * abs(sine)
    return np.clip((across[None, :] * cosine + down[:, None] * sine) / extent + 0.5, 0, 1)


def radial(shape: tuple[int, int], x: float, y: float, radius: float) -> np.ndarray:
    """The distance of a pixel's centre from (`x` of the width, `y` of the height) over `radius` of the shorter side."""
    height, width = shape
    distance = np.hypot(np.arange(width)[None, :] + 0.5 - x * width, np.arange(height)[:, None] + 0.5 - y * height)
    return np.clip(distance / (radius * min(shape)), 0, 1)


def band(depth: np.ndarray, low: float, high: float, feather: float) -> np.ndarray:
    """1 from `low` to `high`, fading linearly to 0 over `feather` beyond either (hard at 0); float32 as the map is."""
    outside = np.maximum(np.float32(low) - depth, depth - np.float32(high))
    if feather == 0:
        return (outside <= 0).astype(np.float32)
    return np.clip(1 - outside / np.float32(feather), 0, 1)


def first_pixel_numbers(raw: np.ndarray) -> np.ndarray:
    """`raw` (a number for each pixel) renumbered 0, 1, ... in the order the numbers first appear in reading order."""
    values, first = np.unique(raw.ravel(), return_index=True)
    rank = {int(value): number for number, value in enumerate(values[np.argsort(first)])}
    return np.vectorize(rank.get)(raw)


def grid_cells(
    shape: tuple[int, int], columns: tuple[float, ...], rows: tuple[float, ...], frame: tuple[int, int, int, int] | None
) -> np.ndarray:
    """The piece of each pixel of a grid: a pixel is past a border when its centre is at or beyond it, the border after
    the first k shares of `columns` being at left + (right - left) x (their sum) / (all of them). Pieces are numbered
    row
    after row among the cells that have a pixel."""
    height, width = shape
    left, top, right, bottom = frame or (0, 0, width, height)

    def past(centres: np.ndarray, low: int, high: int, shares: tuple[float, ...]) -> np.ndarray:
        borders = [low + (high - low) * sum(shares[:k]) / sum(shares) for k in range(1, len(shares))]
        return (
            np.sum([(centres >= border).astype(int) for border in borders], axis=0)
            if borders
            else np.zeros(centres.shape, int)
        )

    across = past(np.arange(width) + 0.5, left, right, columns)
    down = past(np.arange(height) + 0.5, top, bottom, rows)
    return first_pixel_numbers(down[:, None] * len(columns) + across[None, :])


def side_of_line(shape: tuple[int, int], x: float, y: float, degrees: float) -> np.ndarray:
    """The side of the line through (`x` of the width, `y` of the height) at `degrees` clockwise from the right that
    each pixel's centre is on, as pieces numbered by first pixel: the sign of its offset along the line's normal."""
    height, width = shape
    across = np.arange(width)[None, :] + 0.5 - x * width
    down = np.arange(height)[:, None] + 0.5 - y * height
    radians = np.radians(degrees)
    return first_pixel_numbers(down * np.cos(radians) - across * np.sin(radians) > 0)


def bands_of(depth: np.ndarray, count: int) -> np.ndarray:
    """The band of each pixel: the highest first, a value on a border in the lower band, an empty band left out."""
    index = np.minimum(np.floor((1 - depth.astype(np.float64)) * count), count - 1).astype(int)
    return np.unique(index, return_inverse=True)[1].reshape(index.shape)


# --- selections, seen through a mask -------------------------------------------------------------------------------

GRID_3X2 = Grid(columns=3, rows=2)


def cells_3x2(photo: np.ndarray, subject: np.ndarray) -> np.ndarray:
    return grid_cells(photo.shape[:2], (1, 1, 1), (1, 1), None)


def subject_alpha(photo: np.ndarray, subject: np.ndarray) -> np.ndarray:
    return subject


def depth_band(photo: np.ndarray, subject: np.ndarray) -> np.ndarray:
    return band(depth_of(photo), 0.3, 0.7, 0.2)


def gradient(photo: np.ndarray, subject: np.ndarray) -> np.ndarray:
    return linear(photo.shape[:2], 30)


def combined(op: Callable[..., np.ndarray], *alphas: Callable[[np.ndarray, np.ndarray], np.ndarray]):
    def make(photo: np.ndarray, subject: np.ndarray) -> np.ndarray:
        return op(*(alpha(photo, subject).astype(np.float32) for alpha in alphas))

    return make


TRIPLE = ("soft", "tilt", "subject")
COMBINE_REGIONS = {
    "soft": Band(of="depth", low=0.3, high=0.7, feather=0.2),
    "tilt": LinearGradient(angle=30),
}

# (the recipe, the maps handed in besides the subject's and the depth's, the alpha the mask must be)
SELECTIONS: dict[str, tuple[Recipe, Callable[[np.ndarray, np.ndarray], np.ndarray]]] = {
    "a band with hard edges": (
        masked_by("r", r=Band(of="depth", low=0.25, high=0.75, feather=0)),
        lambda photo, subject: band(depth_of(photo), 0.25, 0.75, 0),
    ),
    "a band with soft edges": (masked_by("r", r=Band(of="depth", low=0.3, high=0.7, feather=0.2)), depth_band),
    "a band of the subject": (
        masked_by("r", r=Band(of="subject", low=0.5, high=1, feather=0.5)),
        lambda photo, subject: band(subject, 0.5, 1, 0.5),
    ),
    "a band of a band": (
        masked_by("r", a=Band(of="depth", low=0.2, high=0.8, feather=0.2), r=Band(of="a", low=0.5, high=1, feather=0)),
        lambda photo, subject: band(band(depth_of(photo), 0.2, 0.8, 0.2), 0.5, 1, 0),
    ),
    "the subject region": (masked_by("r", r=Subject()), subject_alpha),
    "combined by multiply": (
        masked_by("r", **COMBINE_REGIONS, r=Combine(op="multiply", of=TRIPLE)),
        combined(lambda *a: a[0] * a[1] * a[2], depth_band, gradient, subject_alpha),
    ),
    "combined by min": (
        masked_by("r", **COMBINE_REGIONS, r=Combine(op="min", of=TRIPLE)),
        combined(lambda *a: np.minimum(np.minimum(a[0], a[1]), a[2]), depth_band, gradient, subject_alpha),
    ),
    "combined by max of two": (
        masked_by("r", **COMBINE_REGIONS, r=Combine(op="max", of=("soft", "tilt"))),
        combined(np.maximum, depth_band, gradient),
    ),
    "a pick of two cells of a 3 by 2 grid": (
        masked_by("r", g=GRID_3X2, r=Pick(of="g", pieces=(0, 4))),
        lambda photo, subject: np.isin(cells_3x2(photo, subject), (0, 4)).astype(np.float32),
    ),
    "a pick of a cell that does not exist": (
        masked_by("r", g=GRID_3X2, r=Pick(of="g", pieces=(9,))),
        lambda photo, subject: np.zeros(photo.shape[:2], np.float32),
    ),
    "a radial gradient": (
        masked_by("r", r=RadialGradient(x=0.4, y=0.6, radius=0.7)),
        lambda photo, subject: radial(photo.shape[:2], 0.4, 0.6, 0.7),
    ),
}


def gradient_at(degrees: float) -> Callable[[np.ndarray, np.ndarray], np.ndarray]:
    return lambda photo, subject: linear(photo.shape[:2], degrees)


for _degrees in (0, 90, 180, 270, 45, -30, 725):
    SELECTIONS[f"a linear gradient at {_degrees} degrees"] = (
        masked_by("r", r=LinearGradient(angle=_degrees)),
        gradient_at(_degrees),
    )


@pytest.mark.parametrize(("recipe", "alpha"), SELECTIONS.values(), ids=SELECTIONS.keys())
def test_a_selection_chooses_what_its_definition_says(photo, subject, recipe, alpha):
    maps = {"subject": subject, "depth": depth_of(photo)}
    made = core_render(photo, recipe, maps=maps)
    np.testing.assert_allclose(made, over(photo, stops(photo, 1), alpha(photo, subject)), atol=2e-6)


def test_a_gradient_turned_a_whole_number_of_turns_is_the_same_gradient(photo):
    five = core_render(photo, masked_by("r", r=LinearGradient(angle=5)))
    np.testing.assert_allclose(core_render(photo, masked_by("r", r=LinearGradient(angle=725))), five, atol=1e-6)


def test_a_band_on_a_border_is_chosen_when_the_map_holds_the_border_as_a_float32_does(photo):
    # 0.7 is not exact in either width: the float32 nearest to it is below the float64 0.7, so against low = 0.7 a
    # double
    # comparison would drop it. A map holds float32; the border is narrowed to it first, so the band chooses the lot.
    depth = np.full(photo.shape[:2], np.float32(0.7))
    recipe = masked_by("r", r=Band(of="depth", low=0.7, high=0.9, feather=0))
    made = core_render(photo, recipe, maps={"depth": depth})
    np.testing.assert_allclose(made, stops(photo, 1), atol=2e-6)  # chosen everywhere


# --- partitions, seen through an effect that paints each pixel with its piece number -------------------------------


def labels_made(photo: np.ndarray, recipe: Recipe, **given: Any) -> np.ndarray:
    """The piece of each pixel as the core's `partition_probe` shows it (the number over 255)."""
    return np.rint(core_render(photo, recipe, **given)[..., 0] * 255).astype(int)


def probing(**regions: Any) -> Recipe:
    """The recipe that shows the pieces of the partition `r`; the effect exists only while the `partition_probe`
    fixture is in use."""
    return Recipe([Step("partition_probe", partition="r", opacity=1)], regions=regions)


def subject_box(subject: np.ndarray) -> tuple[int, int, int, int]:
    rows, columns = np.nonzero(subject > 0.5)
    return int(columns.min()), int(rows.min()), int(columns.max()) + 1, int(rows.max()) + 1


PARTITIONS: dict[str, tuple[dict[str, Any], Callable[[np.ndarray, np.ndarray], np.ndarray]]] = {
    "equal columns and rows": (
        {"r": Grid(columns=3, rows=2)},
        lambda photo, subject: grid_cells(photo.shape[:2], (1, 1, 1), (1, 1), None),
    ),
    "uneven shares": (
        {"r": Grid(columns=(1, 2), rows=(1, 3, 1))},
        lambda photo, subject: grid_cells(photo.shape[:2], (1, 2), (1, 3, 1), None),
    ),
    "a grid laid over the subject's box": (
        {"r": Grid(columns=3, rows=2, fit="subject")},
        lambda photo, subject: grid_cells(photo.shape[:2], (1, 1, 1), (1, 1), subject_box(subject)),
    ),
    "the bands of the depth": (
        {"r": Bands(of="depth", count=3)},
        lambda photo, subject: bands_of(depth_of(photo), 3),
    ),
    "a band that leaves bands empty": (
        {"r": Bands(of="depth", count=100)},
        lambda photo, subject: bands_of(depth_of(photo), 100),
    ),
    "a line through a point at an angle": (
        {"r": Cuts(lines=({"x": 0.4, "y": 0.6, "angle": 30},))},
        lambda photo, subject: side_of_line(photo.shape[:2], 0.4, 0.6, 30),
    ),
    "two lines crossing": (
        {"r": Cuts(lines=({"x": 0.4, "y": 0.6, "angle": 30}, {"x": 0.7, "y": 0.3, "angle": 100}))},
        lambda photo, subject: first_pixel_numbers(
            side_of_line(photo.shape[:2], 0.4, 0.6, 30) + 2 * side_of_line(photo.shape[:2], 0.7, 0.3, 100)
        ),
    ),
    "a straight curve down the picture": (
        {"r": Cuts(lines=({"path": "M 0.25 0 L 0.25 1"},))},
        lambda photo, subject: first_pixel_numbers(np.broadcast_to(np.arange(128)[None, :] + 0.5 > 32, (96, 128))),
    ),
    "a selection used as a partition": (
        {"r": Band(of="depth", low=0.25, high=0.75, feather=0.1)},
        lambda photo, subject: first_pixel_numbers((band(depth_of(photo), 0.25, 0.75, 0.1) > 0.5).astype(int)),
    ),
}


@pytest.mark.parametrize(("regions", "expected"), PARTITIONS.values(), ids=PARTITIONS.keys())
def test_the_pieces_of_a_partition_are_numbered_as_defined(photo, subject, partition_probe, regions, expected):
    recipe = probing(**regions)
    maps = {"subject": subject, "depth": depth_of(photo)}
    np.testing.assert_array_equal(labels_made(photo, recipe, maps=maps), expected(photo, subject))


MOSAIC = ROOT / "examples" / "assets" / "mosaic.svg"
RECTANGLES = (
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 96">'
    '<rect id="a" x="10" y="10" width="40" height="30"/><rect id="b" x="30" y="20" width="50" height="50"/></svg>'
)


def rectangles_made(tmp_path: Path) -> Svg:
    """The svg region of two overlapping rectangles on whole pixels of the 128 x 96 photo (b lies on top of a)."""
    path = tmp_path / "rectangles.svg"
    path.write_text(RECTANGLES, encoding="utf-8")
    return Svg(file=str(path))


def test_the_pieces_of_an_svg_are_numbered_as_defined(photo, partition_probe, tmp_path):
    drawn = np.zeros(photo.shape[:2], int)  # 0 outside, 1 a, 2 b: the later shape lies on top
    drawn[10:40, 10:50] = 1
    drawn[20:70, 30:80] = 2
    recipe = probing(r=rectangles_made(tmp_path))
    np.testing.assert_array_equal(labels_made(photo, recipe), first_pixel_numbers(drawn))


def test_picking_shapes_of_an_svg_by_id_chooses_the_pieces_they_show(photo, tmp_path):
    drawn = np.zeros(photo.shape[:2], bool)  # a without what b covers
    drawn[10:40, 10:50] = True
    drawn[20:70, 30:80] = False
    recipe = masked_by("p", s=rectangles_made(tmp_path), p=Pick(of="s", pieces=("a",)))
    made = core_render(photo, recipe)
    np.testing.assert_allclose(made, over(photo, stops(photo, 1), drawn), atol=2e-6)


def test_an_id_the_svg_lacks_is_refused_listing_the_ids(photo, tmp_path):
    recipe = masked_by("p", s=rectangles_made(tmp_path), p=Pick(of="s", pieces=("c",)))
    with pytest.raises(RecipeError, match=r"region 'p'.*'c'.*'s'.*a, b"):
        core_render(photo, recipe)


def test_a_realistic_svg_is_cut_into_the_outside_and_its_11_shapes(photo, partition_probe):
    recipe = probing(r=Svg(file=str(MOSAIC)))
    assert labels_made(photo, recipe).max() == 11  # piece 0 is the outside


def test_a_torn_svg_is_cut_the_same_for_a_seed_differently_for_another_and_straight_for_no_tear(
    photo, partition_probe, tmp_path
):
    def torn(**edge: Any) -> np.ndarray:
        svg = rectangles_made(tmp_path)
        return labels_made(photo, probing(r=Svg(file=svg.file, edge=Edge(style="torn", **edge))))

    straight = labels_made(photo, probing(r=rectangles_made(tmp_path)))
    first = torn(seed=1, amount=24)
    np.testing.assert_array_equal(first, torn(seed=1, amount=24))
    assert (first != torn(seed=2, amount=24)).any()
    assert (first != straight).any()
    np.testing.assert_array_equal(torn(seed=1, amount=0), straight)


@pytest.mark.parametrize("field", ["columns", "rows"])
def test_shares_that_add_up_to_more_than_the_most_are_refused_naming_the_field_and_huge_ones_within_it_are_fine(
    photo, partition_probe, field
):
    with pytest.raises(RecipeError, match=f"`{field}` shares must add up to at most 1e100"):
        probing(r=Grid(**{field: (1e307, 1e307)}))
    equal = probing(r=Grid(**{field: (5e99, 5e99)}))
    np.testing.assert_array_equal(labels_made(photo, equal), labels_made(photo, probing(r=Grid(**{field: 2}))))


def test_a_pixel_whose_centre_is_on_a_border_is_in_the_later_piece(partition_probe):
    photo = np.zeros(
        (1, 3, 3), np.float32
    )  # two columns of three pixels: the border at 1.5 is the middle pixel's centre
    np.testing.assert_array_equal(labels_made(photo, probing(r=Grid(columns=2))), [[0, 1, 1]])


# --- subject found piece by piece -----------------------------------------------------------------------------------


class Truth:
    """A finder that knows the truth: the red of the crop where it is above 0.5. It keeps the shapes it was asked about
    and finds nothing (raises, as the package's finders do) in a crop without red above `at_least`."""

    def __init__(self, at_least: float = 0.0) -> None:
        self.at_least = at_least
        self.shapes: list[tuple[int, ...]] = []

    def find(self, rgb: np.ndarray) -> np.ndarray:
        self.shapes.append(rgb.shape[:2])
        if not (rgb[..., 0] > self.at_least).any():
            raise SubjectNotFoundError("nothing here")
        return np.where(rgb[..., 0] > 0.5, rgb[..., 0], 0).astype(np.float32)


def found_in_pieces(photo: np.ndarray) -> np.ndarray:
    """The truth over the whole picture: the red where it is above one half."""
    return np.where(photo[..., 0] > 0.5, photo[..., 0], 0)


PER_PIECE = {"r": Grid(columns=2, rows=2), "found": Subject(per="r", context=0.25, threshold=0)}


def test_the_subject_found_in_each_piece_is_united_into_the_truth_whatever_the_pieces(photo):
    finder = Truth()
    recipe = masked_by("found", **PER_PIECE)
    made = core_render(photo, recipe, finders={"subject": finder})
    np.testing.assert_allclose(made, over(photo, stops(photo, 1), found_in_pieces(photo)), atol=2e-6)
    assert len(finder.shapes) == 4  # once for each piece


def test_a_piece_the_finder_finds_nothing_in_adds_nothing_and_the_others_are_unaffected(photo):
    # nothing above 0.9 red in a crop: that piece adds nothing, though it has red above one half
    finder = Truth(at_least=0.9)
    recipe = masked_by("found", **PER_PIECE)
    made = core_render(photo, recipe, finders={"subject": finder})
    height, width = photo.shape[:2]
    pieces = grid_cells((height, width), (1, 1), (1, 1), None)
    expected = np.zeros((height, width))
    for piece in range(4):
        rows, columns = np.nonzero(pieces == piece)
        top, left = (
            max(rows.min() - round(0.25 * (rows.max() + 1 - rows.min())), 0),
            max(columns.min() - round(0.25 * (columns.max() + 1 - columns.min())), 0),
        )
        bottom = min(rows.max() + 1 + round(0.25 * (rows.max() + 1 - rows.min())), height)
        right = min(columns.max() + 1 + round(0.25 * (columns.max() + 1 - columns.min())), width)
        if (photo[top:bottom, left:right, 0] > 0.9).any():
            expected[pieces == piece] = found_in_pieces(photo)[pieces == piece]
    assert expected.any()
    assert not np.array_equal(expected, found_in_pieces(photo))  # some piece really found nothing
    np.testing.assert_allclose(made, over(photo, stops(photo, 1), expected), atol=2e-6)
    traced = bridge.trace(
        photo, recipe, finders={"subject": Truth(at_least=0.9)}
    )  # a trace asks the finder the same way
    np.testing.assert_array_equal(traced[-1][2], made)


@pytest.mark.parametrize(
    ("raised", "error"),
    [(SubjectNotFoundError, None), (DepthNotFoundError, DepthNotFoundError), (MapNotFoundError, MapNotFoundError)],
    ids=["the subject", "the depth", "another map"],
)
def test_only_the_subject_not_being_found_leaves_a_piece_empty_the_others_are_the_renders_error(photo, raised, error):
    class Finds:
        def find(self, rgb: np.ndarray) -> np.ndarray:
            raise raised("not in this piece")

    recipe = masked_by("found", **PER_PIECE)
    if error is None:  # every piece adds nothing: the mask chooses nothing and the photo is as it was
        np.testing.assert_array_equal(core_render(photo, recipe, finders={"subject": Finds()}), photo)
        return
    with pytest.raises(error, match="not in this piece") as caught:
        core_render(photo, recipe, finders={"subject": Finds()})
    assert type(caught.value) is error


def test_an_exception_that_only_looks_like_the_cores_not_found_error_is_a_failure_as_any_other(photo):
    class Looks(ValueError):
        kind = "subject_not_found"

    raised = Looks("a ValueError with a kind of its own")

    class Finds:
        def find(self, rgb: np.ndarray) -> np.ndarray:
            raise raised

    with pytest.raises(Looks) as caught:
        core_render(photo, masked_by("found", **PER_PIECE), finders={"subject": Finds()})
    assert caught.value is raised


def test_another_failure_of_the_finder_is_the_renders_failure_as_it_was_raised(photo):
    class Breaks:
        def find(self, rgb: np.ndarray) -> np.ndarray:
            raise ZeroDivisionError("boom")

    with pytest.raises(ZeroDivisionError, match="boom"):
        core_render(photo, masked_by("found", **PER_PIECE), finders={"subject": Breaks()})


def test_a_subject_found_per_piece_needs_a_finder_and_a_map_of_the_whole_photo_does_not_do(photo, subject):
    with pytest.raises(SubjectNotFoundError, match="finder"):
        core_render(photo, masked_by("found", **PER_PIECE), maps={"subject": subject})


def test_a_depth_finder_that_finds_nothing_is_the_depth_not_found(photo):
    class Nothing:
        def find(self, rgb: np.ndarray) -> np.ndarray:
            raise DepthNotFoundError("no depth here")

    recipe = Recipe([Step("grade", params={"exposure": Drive(by="depth", start=0, end=1)})])
    with pytest.raises(DepthNotFoundError, match="no depth here"):
        core_render(photo, recipe, finders={"depth": Nothing()})


# --- each --------------------------------------------------------------------------------------------------------

LOOKS = (DOUBLED, None, QUADRUPLED)


def each_recipe(region: Any, feather: float, **each: Any) -> Recipe:
    return Recipe([Each(each="r", looks=LOOKS, feather=feather, **each)], regions={"r": region})


def blended(photo: np.ndarray, pieces: np.ndarray, assignment: list[int], feather: float) -> np.ndarray:
    """`photo` with each piece shown as its look says: the weights are the indicator of the look's pieces blurred by a
    Gaussian of `feather` design pixels (the edge repeated, cut at 4 sigmas), and the picture is the photo plus each
    look's weighted difference. The looks are `DOUBLED`, none, `QUADRUPLED`."""
    sigma = feather * max(photo.shape[:2]) / 480
    results = {0: stops(photo, 1), 2: stops(photo, 2)}
    out = photo.astype(np.float64)
    for look, result in results.items():
        weight = np.isin(pieces, [piece for piece, given in enumerate(assignment) if given == look]).astype(np.float32)
        if sigma > 0:
            weight = ndimage.gaussian_filter(weight, sigma, mode="nearest", truncate=4)
        out += weight[..., None] * (result - photo)
    return np.clip(out, 0, 1)


# the core feathers by three box blurs, not scipy's Gaussian: weights differ by up to 0.035, pictures by up to 0.021
FEATHER_ATOL = 3e-2

EACH_PIECES = {
    "columns": (Grid(columns=5), lambda photo, subject: grid_cells(photo.shape[:2], (1,) * 5, (1,), None)),
    "bands of the depth": (Bands(of="depth", count=4), lambda photo, subject: bands_of(depth_of(photo), 4)),
}


@pytest.mark.parametrize("scale", [1, 2], ids=["the photo", "twice the photo"])
@pytest.mark.parametrize("feather", [0, 0.5, 5])
@pytest.mark.parametrize(("region", "pieces"), EACH_PIECES.values(), ids=EACH_PIECES.keys())
def test_each_gives_the_pieces_their_looks_in_turn_blended_across_the_borders(
    photo, subject, region, pieces, feather, scale
):
    photo = photo.repeat(scale, axis=0).repeat(scale, axis=1)
    subject = subject.repeat(scale, axis=0).repeat(scale, axis=1)
    recipe = each_recipe(region, feather)
    maps = {"subject": subject, "depth": depth_of(photo)}
    labels = pieces(photo, subject)
    made = core_render(photo, recipe, maps=maps)
    assignment = [piece % 3 for piece in range(labels.max() + 1)]
    np.testing.assert_allclose(made, blended(photo, labels, assignment, feather), atol=FEATHER_ATOL)


def test_each_with_hard_borders_leaves_the_piece_of_a_none_look_exactly_as_it_was(photo):
    made = core_render(photo, each_recipe(Grid(columns=3), 0))
    middle = (np.arange(photo.shape[1]) + 0.5 >= 128 / 3) & (np.arange(photo.shape[1]) + 0.5 < 256 / 3)
    np.testing.assert_array_equal(made[:, middle], photo[:, middle])  # 128 pixels: the borders are at 42.67 and 85.33
    assert middle.sum() == 42
    assert not np.array_equal(made[:, ~middle], photo[:, ~middle])


def test_each_with_looks_that_all_make_white_gives_white_whatever_the_feather(photo):
    # eight stops make white of anything above 1/256: the weights of the looks add up to 1 at every pixel, at the most
    # feather too
    paint = Step("grade", params={"exposure": 8}, opacity=1)
    recipe = Recipe([Each(each="r", looks=(paint, paint), feather=24)], regions={"r": Grid(columns=7)})
    made = core_render(np.clip(photo, 0.05, 1), recipe)
    np.testing.assert_allclose(made, 1.0, atol=2e-6)


def with_seed(seed: int) -> Recipe:
    return Recipe(
        [Each(each="r", looks=(DOUBLED, None, QUADRUPLED), feather=0, assign="random", seed=seed)],
        regions={"r": Grid(columns=24)},
    )


def look_of_each_column(photo: np.ndarray, made: np.ndarray) -> list[int]:
    """Which look (0 doubled, 1 none, 2 quadrupled) each of the 24 columns shows; fails unless each shows one."""
    outcomes = (stops(photo, 1), photo, stops(photo, 2))
    columns = grid_cells(photo.shape[:2], (1,) * 24, (1,), None)[0]
    shown = []
    for column in range(24):
        fits = [
            np.allclose(made[:, columns == column], outcome[:, columns == column], atol=1e-6) for outcome in outcomes
        ]
        shown.append(fits.index(True))
    return shown


def test_a_random_assignment_gives_each_piece_exactly_one_look_the_same_seed_the_same_picture(photo):
    photo = np.clip(photo, 0.05, 0.45)  # no look clips: the three outcomes differ everywhere
    first = core_render(photo, with_seed(1))
    np.testing.assert_array_equal(first, core_render(photo, with_seed(1)))
    assert not np.array_equal(first, core_render(photo, with_seed(2)))
    assert len(look_of_each_column(photo, first)) == 24  # every column is one look, whole


def test_a_random_assignment_uses_every_look_and_favours_none_over_many_seeds(photo):
    photo = np.clip(photo, 0.05, 0.45)
    counts = [0, 0, 0]
    for seed in range(10):
        for look in look_of_each_column(photo, core_render(photo, with_seed(seed))):
            counts[look] += 1
    assert all(55 <= count <= 105 for count in counts), counts  # 240 draws: 80 each when fair, give or take 3 sigma


# --- the shipped looks and examples -------------------------------------------------------------------------------


def used_regions(recipe: Recipe) -> list[str]:
    """The regions the steps use, or that those are made from."""
    todo = [name for step in recipe.steps for _, name, _ in references(step)]
    names: set[str] = set()
    while todo:
        name = todo.pop()
        if name in names or name not in recipe.regions:
            continue
        names.add(name)
        for field in ("of", "per", "fit"):  # the fields of a region that name the regions it is made from
            made_from = getattr(recipe.regions[name], field, None)
            todo += [made_from] if isinstance(made_from, str) else list(made_from or ())
    return [name for name in recipe.regions if name in names]


def declares_regions(recipe: Recipe) -> bool:
    return bool(recipe.regions) or any(isinstance(step, Each) for step in recipe.steps)


def seeded(recipe: Recipe) -> bool:
    """Whether a region the steps use is shards or torn: both draw from the core's own stream, not numpy's."""
    return any(
        isinstance(recipe.regions[name], Shards) or getattr(recipe.regions[name], "edge", Edge()).style == "torn"
        for name in used_regions(recipe)
    )


def draws_svg(recipe: Recipe) -> bool:
    """Whether a region the steps use is read from an svg file (the core reads it with its own reader)."""
    return any(isinstance(recipe.regions[name], Svg) for name in used_regions(recipe))


RUNNABLE = [path for path in FILES if declares_regions(load_recipe(path))]
DRAWN = [path for path in RUNNABLE if draws_svg(load_recipe(path))]
MADE = [path for path in RUNNABLE if not seeded(load_recipe(path)) and path not in DRAWN]
SEEDED = [path for path in RUNNABLE if seeded(load_recipe(path))]


def ID(path: Path) -> str:
    return str(path.relative_to(ROOT))


def test_the_files_that_use_the_regions_the_core_makes_are_the_ones_the_scan_found():
    assert {path.stem for path in MADE} >= {
        "tilt-shift",
        "tonal-grade",
        "shadow-grain",
        "depth-grade",
        "depth-haze",
        "one-cell",
        "focus_blur",
        "bezier-shape",
        "diagonal",
        "bezier-cut",
    }
    assert {path.stem for path in SEEDED} >= {"tear", "glass", "torn"}
    assert {path.stem for path in DRAWN} >= {"svg"}


@pytest.mark.parametrize("path", MADE, ids=ID)
def test_a_shipped_file_that_uses_regions_renders_a_picture_that_the_look_changed(path, photo, subject):
    recipe = load_recipe(path)
    maps = {"subject": subject, "depth": depth_of(photo)}
    made = core_render(photo, recipe, maps=maps)
    assert made.dtype == np.float32
    assert np.isfinite(made).all()
    assert made.min() >= 0
    assert made.max() <= 1
    last = bridge.trace(photo, recipe, maps=maps)[-1]
    np.testing.assert_array_equal(last[2], made)  # the last stage of a trace is the render
    if recipe.frame == Framing() and recipe.size is None:
        assert np.abs(made - photo).max() > 1e-3  # the look does something


@pytest.mark.parametrize("path", DRAWN, ids=ID)
def test_a_shipped_file_that_draws_an_svg_renders_the_same_every_time_and_stays_a_picture(path, photo, subject):
    recipe = load_recipe(path)
    maps = {"subject": subject, "depth": depth_of(photo)}
    made = core_render(photo, recipe, maps=maps)
    assert made.dtype == np.float32
    assert np.isfinite(made).all()
    assert made.min() >= 0
    assert made.max() <= 1
    np.testing.assert_array_equal(bridge.trace(photo, recipe, maps=maps)[-1][2], made)
    np.testing.assert_array_equal(core_render(photo, recipe, maps=maps), made)


@pytest.mark.parametrize("path", SEEDED, ids=ID)
def test_a_shipped_file_with_shards_or_a_torn_edge_renders_the_same_every_time_and_does_something(path, photo):
    # seeded: the shards and the tear are not numpy's, so only properties are compared (the python engine's differ)
    recipe = load_recipe(path)
    given = {"maps": {"depth": depth_of(photo)}, "finders": {"subject": Truth()}}
    made = core_render(photo, recipe, **given)
    assert made.dtype == np.float32
    assert np.isfinite(made).all()
    assert made.min() >= 0
    assert made.max() <= 1
    np.testing.assert_array_equal(core_render(photo, recipe, **given), made)
    if recipe.frame == Framing() and recipe.size is None:
        assert np.abs(made - photo).max() > 1e-3


def test_a_torn_grid_is_cut_by_the_core_the_same_for_a_seed_differently_for_another_and_straight_for_no_tear(
    photo, partition_probe
):
    def torn(**edge: Any) -> np.ndarray:
        return labels_made(photo, probing(r=Grid(columns=3, rows=2, edge=Edge(style="torn", **edge))))

    straight = labels_made(photo, probing(r=Grid(columns=3, rows=2)))
    first = torn(seed=1, amount=24)
    np.testing.assert_array_equal(first, torn(seed=1, amount=24))
    assert (first != torn(seed=2, amount=24)).any()
    assert (first != straight).any()
    np.testing.assert_array_equal(torn(seed=1, amount=0), straight)


def test_shards_are_at_most_as_many_pieces_as_asked_in_reading_order_the_same_for_a_seed_and_not_for_another(
    photo, partition_probe
):
    def shards(**fields: Any) -> np.ndarray:
        return labels_made(photo, probing(r=Shards(**fields)))

    first = shards(count=9, seed=1)
    assert 1 < first.max() + 1 <= 9
    np.testing.assert_array_equal(first, first_pixel_numbers(first))
    np.testing.assert_array_equal(first, shards(count=9, seed=1))
    assert (first != shards(count=9, seed=2)).any()
    assert shards(count=1).max() == 0
    assert shards(preset="mosaic", seed=3).max() + 1 > 40  # 70 tiles on 96 x 128 px


# --- what a recipe needs of the photo -------------------------------------------------------------------------------

SKY = Given()
NEEDS: dict[str, tuple[Recipe, set[str], bool]] = {
    "a plain recipe": (Recipe([DOUBLED]), set(), False),
    "a mask of the subject": (Recipe([Step("grade", mask="subject")]), {"subject"}, False),
    "a mask of the background": (Recipe([Step("grade", mask="background")]), {"subject"}, False),
    "a mask of the light": (Recipe([Step("grade", mask="light")]), set(), False),
    "a band of the depth": (masked_by("r", r=Band(of="depth")), {"depth"}, False),
    "a band of a band of the depth, and a region declared and not used": (
        masked_by("b", a=Band(of="depth"), b=Band(of="a"), unused=Band(of="subject")),
        {"depth"},
        False,
    ),
    "a given region used and another not": (masked_by("sky", sky=SKY, spare=Given()), {"sky"}, False),
    "a subject region without per": (masked_by("r", r=Subject()), {"subject"}, False),
    "a subject region found per piece": (masked_by("found", **PER_PIECE), set(), True),
    "a grid laid over the subject": (
        Recipe([Each(each="g", looks=(DOUBLED,))], regions={"g": Grid(columns=2, fit="subject")}),
        {"subject"},
        False,
    ),
    "an each over the bands of the depth": (
        Recipe([Each(each="g", looks=(None,))], regions={"g": Bands(of="depth", count=3)}),
        {"depth"},
        False,
    ),
    "a look masked by a given region": (
        Recipe([Each(each="g", looks=(Step("grade", mask="sky"),))], regions={"g": Grid(columns=2), "sky": SKY}),
        {"sky"},
        False,
    ),
    "a frame that crops around the subject": (
        Recipe([DOUBLED], frame=Framing(aspect=1, anchor="subject")),
        {"subject"},
        False,
    ),
}


@pytest.mark.parametrize(("recipe", "maps", "finds"), NEEDS.values(), ids=NEEDS.keys())
def test_what_a_recipe_needs_is_the_maps_and_finders_its_steps_use(recipe, maps, finds):
    assert (set(bridge.maps_needed(recipe)), bridge.needs_finder(recipe)) == (maps, finds)


def test_the_names_a_photo_can_be_given_are_the_subject_the_depth_and_the_given_regions_in_declaration_order():
    recipe = Recipe(
        [DOUBLED], regions={"zeta": Given(), "band": Band(of="depth"), "alpha": Given(), "cells": Grid(columns=2)}
    )
    assert bridge.known_maps(recipe) == ["subject", "depth", "zeta", "alpha"]
    assert bridge.known_maps(Recipe([DOUBLED])) == ["subject", "depth"]


# --- region stages in a trace ------------------------------------------------------------------------------------


def region_stages(stages: list) -> list[tuple[str, str]]:
    """(kind, name) of each region stage of the core's trace, in order."""
    return [(kind, name) for kind, name, _ in stages if kind in ("selection", "partition")]


TRACED = {
    "regions in no order of the alphabet": (
        Recipe(
            [Step("grade", params={"exposure": 1}, mask="both"), Each(each="mid", looks=(DOUBLED, None))],
            regions={
                "zeta": Band(of="depth", low=0.5, high=1, feather=0.25),
                "mid": Grid(columns=3),
                "alpha": LinearGradient(angle=0),
                "both": Combine(op="multiply", of=("alpha", "zeta")),
                "unused": RadialGradient(),
            },
        ),
        [("selection", "zeta"), ("partition", "mid"), ("selection", "alpha"), ("selection", "both")],
    ),
    "a given region": (
        Recipe([Step("grade", mask="sky")], regions={"sky": Given(), "spare": Given()}),
        [("selection", "sky")],
    ),
}


@pytest.mark.parametrize(("recipe", "shown"), TRACED.values(), ids=TRACED.keys())
def test_a_trace_shows_the_regions_the_steps_use_in_the_order_they_are_declared(photo, recipe, shown):
    sky = np.full(photo.shape[:2], 0.25, np.float32)
    maps = {"depth": depth_of(photo), "sky": sky} if "sky" in recipe.regions else {"depth": depth_of(photo)}
    stages = bridge.trace(photo, recipe, maps=maps)
    assert region_stages(stages) == shown
    for kind, name, array in stages:
        if kind == "selection":
            assert array.dtype == np.float32
            assert array.shape == photo.shape[:2]
            assert 0 <= array.min() <= array.max() <= 1
            if name == "sky":
                np.testing.assert_array_equal(array, sky)  # a given map is shown as it was given
        if kind == "partition":
            assert array.dtype == np.int32
            assert array.shape == photo.shape[:2]
    assert [kind for kind, _, _ in stages][-1] == "step"


def test_the_stage_of_a_partition_holds_the_labels_of_its_pieces(photo):
    recipe = Recipe([Each(each="r", looks=(DOUBLED, None))], regions={"r": Grid(columns=3, rows=2)})
    stages = bridge.trace(photo, recipe)
    (labels,) = [array for kind, name, array in stages if (kind, name) == ("partition", "r")]
    np.testing.assert_array_equal(labels, grid_cells(photo.shape[:2], (1, 1, 1), (1, 1), None))
