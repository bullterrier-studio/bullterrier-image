"""The validated base of a recipe's steps and recipes: frozen, unknown fields refused, a failure one `RecipeError`.

Transitional: the Pydantic base that `Step`, `Each` and `Recipe` still stand on, until they are frozen dataclasses over
the core's checks (E7 batch 6.5). Nothing else uses it.

Dependency direction: imports `errors` only.
"""

import difflib
from collections.abc import Iterable, Mapping
from typing import Any, Self

from pydantic import BaseModel, ConfigDict, ValidationError

from bullterrier_image.errors import RecipeError


def describe_error(error: ValidationError, owner: str, valid: Iterable[str]) -> str:
    """One sentence for a failed validation: `owner` (e.g. "effect 'bloom'"; "" for none), then each problem with its
    field, the constraint and the value given. `valid` names the fields, listed when an unknown one was given."""
    problems = "; ".join(_problem(detail, sorted(valid)) for detail in error.errors())
    return f"{owner}: {problems}" if owner else problems


def _invalid(error: ValidationError, owner: str, valid: Iterable[str]) -> RecipeError:
    """The `RecipeError` for a failed validation, in the wording of `describe_error`. Pydantic wraps a `ValueError`
    raised inside a validator; when that is the only problem and it is a more specific `RecipeError` (an unknown effect
    is also a `KeyError`), the caller gets that class back, so `except UnknownEffectError` works on a `Step` too."""
    problems = [detail.get("ctx", {}).get("error") for detail in error.errors()]
    single = problems[0] if len(problems) == 1 else None
    kind = type(single) if isinstance(single, RecipeError) else RecipeError
    return kind(describe_error(error, owner, valid))


class ValidatedModel(BaseModel):
    """Base of bullterrier-image's validated values (effect parameters, recipe steps, recipes): frozen, unknown fields
    refused,
    and every validation failure reported as one `RecipeError` in the wording of `describe_error`."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    @classmethod
    def parse(cls, values: Mapping[str, object], owner: str = "") -> Self:
        """The model for `values` (field name -> value); RecipeError naming `owner` (e.g. "effect 'bloom'"), each bad
        field, the constraint and the value, and listing the fields when an unknown one was given."""
        try:
            return super().model_validate(dict(values))
        except ValidationError as error:
            raise _invalid(error, owner, cls.model_fields) from None

    @classmethod
    def model_validate(cls, obj: Any, **options: Any) -> Self:
        """pydantic's `model_validate`, its failure a RecipeError as `parse` reports it."""
        try:
            return super().model_validate(obj, **options)
        except ValidationError as error:
            raise _invalid(error, "", cls.model_fields) from None

    @classmethod
    def model_validate_json(cls, json_data: str | bytes | bytearray, **options: Any) -> Self:
        """pydantic's `model_validate_json`, its failure a RecipeError as `parse` reports it."""
        try:
            return super().model_validate_json(json_data, **options)
        except ValidationError as error:
            raise _invalid(error, "", cls.model_fields) from None

    @classmethod
    def model_validate_strings(cls, obj: Any, **options: Any) -> Self:
        """pydantic's `model_validate_strings`, its failure a RecipeError as `parse` reports it."""
        try:
            return super().model_validate_strings(obj, **options)
        except ValidationError as error:
            raise _invalid(error, "", cls.model_fields) from None

    @classmethod
    def model_construct(cls, _fields_set: set[str] | None = None, **values: Any) -> Self:  # type: ignore[override]  # mypy reads pydantic's underscore name as private
        """Refused: pydantic's `model_construct` builds a value without checking it, which nothing may. Make one with
        its constructor or `model_validate`."""
        raise RecipeError(
            f"{cls.__name__}.model_construct() builds a value without checking it: use {cls.__name__}(...) or "
            f"{cls.__name__}.model_validate(...)"
        )

    def model_copy(self, *, update: Mapping[str, Any] | None = None, deep: bool = False) -> Self:
        """A copy with `update` laid over the values, checked as a new value is (pydantic's own copy checks nothing):
        a bad value is a RecipeError. Without `update` the copy is equal (with `deep`, of copied fields)."""
        if not update:
            return super().model_copy(deep=deep)
        return type(self).model_validate({**dict(self), **update})

    def _init_checked(self, owner: str, /, **values: Any) -> None:
        """For a subclass's own (positional) constructor: validate `values` into this instance; RecipeError like
        `parse`."""
        try:
            BaseModel.__init__(self, **values)
        except ValidationError as error:
            raise _invalid(error, owner, type(self).model_fields) from None


def _number(value: float) -> str:
    """A bound as a person writes it: 0, 500, 0.5, 4294967295."""
    return str(int(value)) if float(value).is_integer() and abs(value) < 1e15 else repr(value)


def _nearest(given: object, valid: list[str]) -> str:
    """ " (did you mean `x`?)" for the name `x` in `valid` nearest the misspelt `given`; "" when none is near."""
    near = difflib.get_close_matches(str(given), valid, n=1)
    return f" (did you mean `{near[0]}`?)" if near else ""


def _problem(detail: Any, valid: list[str]) -> str:
    location = detail["loc"]
    if not location:  # a rule about the whole model, not one field
        return str(detail.get("ctx", {}).get("error") or detail["msg"])
    name = "`" + "".join(f"[{part}]" if isinstance(part, int) else f".{part}" for part in location).lstrip(".") + "`"
    kind, ctx, given = detail["type"], detail.get("ctx", {}), detail.get("input")
    got = f", got {given!r}"
    wording = {
        "extra_forbidden": f"{name} is not a parameter{_nearest(location[-1], valid)}; the parameters are: "
        f"{', '.join(valid)}",
        "missing": f"{name} is required",
        "greater_than_equal": f"{name} must be >= {_number(ctx.get('ge', 0))}{got}",
        "less_than_equal": f"{name} must be <= {_number(ctx.get('le', 0))}{got}",
        "greater_than": f"{name} must be > {_number(ctx.get('gt', 0))}{got}",
        "less_than": f"{name} must be < {_number(ctx.get('lt', 0))}{got}",
        "float_type": f"{name} must be a number{got}",
        "int_type": f"{name} must be a whole number{got}",
        "bool_type": f"{name} must be true or false{got}",
        "string_type": f"{name} must be text{got}",
        "tuple_type": f"{name} must be a list{got}",
        "finite_number": f"{name} must be a finite number{got}",
        "literal_error": f"{name} must be one of {ctx.get('expected')}{got}",
        "too_short": f"{name} needs at least {ctx.get('min_length')} items{got}",
        "string_pattern_mismatch": f"{name} must match the pattern {ctx.get('pattern')}{got}",
        "value_error": f"{name}: {ctx.get('error')}",
    }
    return wording.get(kind, f"{name}: {detail['msg']}{got}")
