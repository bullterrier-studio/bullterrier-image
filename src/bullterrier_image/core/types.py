"""Types that are part of the effect contract itself, shared by effects and the recipe machinery."""

# A colour as a recipe or an `fx` call may write it: '#rrggbb' / '#rgb', or [r, g, b] with values 0..1.
Color = str | list[float] | tuple[float, float, float]
