"""Finding a map in a photo: the subject's cut-out and the depth, with the platform's segmenter or portable models.

`subject` finds the subject (`subject_matte`, `subject_finder`, `refine_edge`); `depth` finds how near each pixel is
(`depth_map`, `depth_finder`, `inverse_depth`). The work is the core's (`bullterrier_image.finding` in Rust): these
modules name it for Python. The ONNX Runtime library and the models are never part of the package and never fetched at
import time; `bt-image install onnx` fetches them.
"""
