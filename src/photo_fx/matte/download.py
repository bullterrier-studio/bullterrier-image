"""Getting a model file onto this computer: downloaded once, verified, and kept in the user's cache folder.

`fetch(url, sha256, target)` promises that it returns only a file whose SHA-256 is the one asked for, and that it
never leaves a partial or unverified file at `target` (or beside it), whatever goes wrong: a dropped connection, a
full disk, a server sending something else. A file already at `target` with the right digest is used without touching
the network. Nothing here runs at import time.

`user_cache_dir()` is where such files belong on each platform.
"""

import hashlib
import http.client
import os
import sys
import tempfile
import urllib.request
from collections.abc import Mapping
from pathlib import Path

APP = "photo-fx"
_CHUNK = 1 << 20  # bytes read at a time: memory use only, the result does not depend on it
_TIMEOUT = 60.0  # seconds a stalled connection may stay silent before the download fails

# (path, size, modification time, digest) of files already verified in this process: a 180 MB model is hashed once,
# not on every call
_verified: set[tuple[Path, int, int, str]] = set()


class DownloadError(RuntimeError):
    """A model file could not be downloaded or did not verify; the message says how to get it by hand."""


def fetch(url: str, sha256: str, target: Path) -> Path:
    """Return `target`, downloading it from `url` first unless it is already there with SHA-256 `sha256`.

    url: where the file is (`https://`, `http://` or `file://`). sha256: its expected SHA-256, lowercase hex.
    target: where it is kept; missing folders are created. A file at `target` with another digest (an earlier broken
    copy) is replaced. Raises DownloadError, naming `url` and `target` so the file can be fetched by hand, when the
    download fails or what arrived has another digest; `target` is then left as it was and no temporary file remains.
    """
    target = Path(target)
    if _is_verified(target, sha256):
        return target
    target.parent.mkdir(parents=True, exist_ok=True)
    by_hand = f"download {url} by hand and save it as {target}"
    descriptor, temporary = tempfile.mkstemp(prefix=f".{target.name}.", suffix=".part", dir=target.parent)
    try:
        digest = hashlib.sha256()
        with os.fdopen(descriptor, "wb") as out:  # closed before the rename below, which Windows requires
            try:
                with urllib.request.urlopen(url, timeout=_TIMEOUT) as response:
                    expected_size = response.headers.get("Content-Length")
                    received = 0
                    while chunk := response.read(_CHUNK):
                        out.write(chunk)
                        digest.update(chunk)
                        received += len(chunk)
            except (OSError, http.client.HTTPException) as error:  # URLError, timeouts and resets are OSErrors
                raise DownloadError(f"could not download {url} ({error}); {by_hand}") from error
        if expected_size is not None and received != int(expected_size):
            raise DownloadError(
                f"the download of {url} stopped after {received} of {expected_size} bytes; try again, or {by_hand}"
            )
        if digest.hexdigest() != sha256:
            raise DownloadError(
                f"the file downloaded from {url} has SHA-256 {digest.hexdigest()}, but {sha256} was expected, so it "
                f"was not saved; {by_hand} (and check that it is the right file)"
            )
        os.replace(temporary, target)
    except BaseException:
        Path(temporary).unlink(missing_ok=True)
        raise
    _verified.add(_fingerprint(target, sha256))
    return target


def user_cache_dir(
    platform: str = sys.platform, environ: Mapping[str, str] = os.environ, home: Path | None = None
) -> Path:
    """The folder photo-fx keeps downloaded files in, following the platform's convention.

    macOS `~/Library/Caches/photo-fx`; Windows `%LOCALAPPDATA%/photo-fx/Cache`; Linux and others
    `$XDG_CACHE_HOME/photo-fx`, or `~/.cache/photo-fx` when that is unset or empty. platform: a `sys.platform` value.
    environ: the environment variables to read. home: the user's home folder (default `Path.home()`).
    """
    home = Path.home() if home is None else home
    if platform == "darwin":
        return home / "Library" / "Caches" / APP
    if platform == "win32":
        local = environ.get("LOCALAPPDATA") or str(home / "AppData" / "Local")
        return Path(local) / APP / "Cache"
    return Path(environ.get("XDG_CACHE_HOME") or home / ".cache") / APP


def _is_verified(path: Path, sha256: str) -> bool:
    if not path.is_file():
        return False
    if _fingerprint(path, sha256) in _verified:
        return True
    digest = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(_CHUNK):
            digest.update(chunk)
    if digest.hexdigest() != sha256:
        return False
    _verified.add(_fingerprint(path, sha256))
    return True


def _fingerprint(path: Path, sha256: str) -> tuple[Path, int, int, str]:
    stat = path.stat()
    return path.resolve(), stat.st_size, stat.st_mtime_ns, sha256
