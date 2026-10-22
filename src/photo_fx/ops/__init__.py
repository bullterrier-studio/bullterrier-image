"""Shared building blocks for effects: colour spaces, noise, edges, warping, glyphs, pieces, distance, Voronoi, sprites.

Not effects themselves: an effect is a function on a picture that recipes can name; these are the tools several
effects share, each a small module with one job. Effects depend on `ops`, never the reverse. Import a module by its
path (`from photo_fx.ops import noise`); this file deliberately imports nothing, so adding a module edits no shared file.
"""
