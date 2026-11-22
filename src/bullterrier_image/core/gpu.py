"""`Gpu`: the Rust core's GPU context, which a render is given as `gpu=`.

Interface:
    Gpu()            -> a context on this machine's GPU; GpuError when it has none
    held(gpu)        -> the core's context of a `Gpu`, or None: what `render` hands the core
"""

from typing import Any

from . import native


class Gpu:
    """A GPU context on this machine's high-performance GPU (Metal, Vulkan or Direct3D 12): render on it with
    `render(..., gpu=gpu)`, which returns the same numpy picture as on the CPU, held to each GPU kernel's stated bound
    instead of the CPU's bytes (the CPU stays the exact reference). Share one context between renders and threads.

    `Gpu()` takes the device; a machine without a GPU is a GpuError naming why. `adapter` names the GPU,
    `largest_picture` is the most pixels a picture on it can have, `transfers` counts the pictures sent to it and read
    back (`{"uploads", "downloads", "bytes"}`; an all-GPU recipe makes one of each), `reset_transfers()` sets that count
    to zero.
    """

    __slots__ = ("_core",)

    def __init__(self) -> None:
        """A context on a device of its own."""
        self._core = native.gpu()

    @property
    def adapter(self) -> str:
        """The adapter the device is on, as its driver names it: name, backend, kind and driver."""
        return str(self._core.adapter)

    @property
    def largest_picture(self) -> int:
        """The most pixels a picture on this device can have; a larger render is a GpuError (the CPU renders it)."""
        return int(self._core.largest_picture)

    @property
    def transfers(self) -> dict[str, int]:
        """What has crossed between the CPU and this context since it was made or last reset: pictures and maps sent
        (`uploads`), pictures read back (`downloads`) and the `bytes` of both."""
        return dict(self._core.transfers)

    def reset_transfers(self) -> None:
        """Sets the count of `transfers` back to zero."""
        self._core.reset_transfers()

    def __repr__(self) -> str:
        """The context by its adapter."""
        return f"Gpu(adapter={self.adapter!r})"


def held(gpu: Gpu | None) -> Any:
    """The core's context of `gpu`, or None for none."""
    return None if gpu is None else gpu._core  # noqa: SLF001  (the facade hands the core its own object)
