"""Finding a map in a photo: the subject's cut-out and the depth, with portable models and the platform backend.

`subject` finds the subject (`subject_matte`, `subject_finder`, `refine_edge`); `depth` finds how near each pixel is
(`depth_map`, `depth_finder`); `download` fetches and verifies a model file on first use. Models are never part of the
package and never fetched at import time.
"""
