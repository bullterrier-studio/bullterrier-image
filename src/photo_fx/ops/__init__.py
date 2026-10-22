"""Shared building blocks for effects and core: colour, resampling, glyphs, text art, pieces, distance.

Not effects themselves: an effect is a function on a picture that recipes can name; these are the tools several
effects share, each a small module with one job. Effects depend on `ops`, never the reverse. Import a module by its
path (`from photo_fx.ops import cells`); this file deliberately imports nothing, so adding a module edits no
shared file. Internal, not API: its modules change whenever their consumers do. A module arrives together with its
first consumer and leaves with its last (`uv run vulture` fails on code nothing uses).
"""
