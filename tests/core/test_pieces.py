"""The pieces (regions, the frame, a drive) are dataclasses over the core's normal form: they hold exactly the fields
the core answers with, and a field is required in Python exactly when the core refuses its absence."""

from dataclasses import MISSING, fields

import pytest

from bullterrier_image.core import native
from bullterrier_image.core.pieces import Drive, plain
from bullterrier_image.recipe import (
    Band,
    Bands,
    Combine,
    Curve,
    Cuts,
    Edge,
    Framing,
    Given,
    Grid,
    Line,
    LinearGradient,
    Path,
    Pick,
    RadialGradient,
    Shards,
    Subject,
    Svg,
)

DRIVE = {"by": "near", "start": 0, "end": 1}
# Each piece, with how the core makes it from fields (the oracle, independent of the class) and what it needs at least.
PIECES = {
    "Framing": (Framing, native.frame, {}),
    "Drive": (Drive, native.drive, DRIVE),
    "Edge": (Edge, lambda f: native.region("grid", {"edge": f})["edge"], {}),
    "Line": (Line, lambda f: native.region("cuts", {"lines": [f]})["lines"][0], {}),
    "Curve": (Curve, lambda f: native.region("cuts", {"lines": [{"path": "M 0 0 L 1 1", **f}]})["lines"][0], {}),
    **{
        cls.__name__: (cls, lambda f, kind=kind: native.region(kind, f), {})
        for kind, cls in {
            "grid": Grid,
            "shards": Shards,
            "pick": Pick,
            "cuts": Cuts,
            "path": Path,
            "svg": Svg,
            "subject": Subject,
            "band": Band,
            "bands": Bands,
            "combine": Combine,
            "linear_gradient": LinearGradient,
            "radial_gradient": RadialGradient,
            "given": Given,
        }.items()
    },
}


@pytest.mark.parametrize(("cls", "normal", "needs"), PIECES.values(), ids=PIECES.keys())
def test_a_piece_has_a_field_for_each_key_of_the_cores_normal_form_and_no_other(cls, normal, needs):
    assert set(normal(needs)) == {each.name for each in fields(cls)}


@pytest.mark.parametrize(("cls", "normal", "needs"), PIECES.values(), ids=PIECES.keys())
def test_a_field_is_required_exactly_when_the_core_refuses_its_absence(cls, normal, needs):
    required = {each.name for each in fields(cls) if each.default is MISSING}
    assert required == set(needs)
    cls(**needs)  # built with only what is required, the core accepts it
    full = normal(needs)
    refused = set()
    for key in full:
        try:
            normal({k: v for k, v in plain(cls(**needs)).items() if k != key})
        except ValueError:
            refused.add(key)
    assert refused == required
    for key in required:
        with pytest.raises(TypeError, match=key):
            cls(**{k: v for k, v in needs.items() if k != key})
