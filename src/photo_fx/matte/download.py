"""Getting a model file onto this computer: downloaded once, verified, and kept in the user's cache folder.

`fetch(url, sha256, target)` promises that it returns only a file whose SHA-256 is the one asked for, and that it
never leaves a partial or unverified file at `target` (or beside it), whatever goes wrong: a dropped connection, a
full disk, a server sending something else. A file already at `target` with the right digest is used without touching
the network. It talks to other computers over https only, redirects included; plain http and `file://` are for this
computer (tests, a local mirror). Nothing here runs at import time.

`user_cache_dir()` is where such files belong on each platform.
"""

import glob
import hashlib
import http.client
import os
import secrets
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Mapping
from pathlib import Path
from typing import IO

from photo_fx.errors import PhotoFxError

APP = "photo-fx"
_CHUNK = 1 << 20  # bytes read at a time: memory use only, the result does not depend on it
_TIMEOUT = 60.0  # seconds a stalled connection may stay silent before the download fails
_LOOPBACK = {"127.0.0.1", "::1", "localhost"}  # this computer: the IPv4 and IPv6 loopback addresses and their name
_DEAD_AFTER = 24 * 3600  # seconds since a temporary file was last written after which its download is surely dead
_NEW_FILE = 0o666  # the permissions a new file asks for before the umask (POSIX open())
_BINARY = getattr(os, "O_BINARY", 0)  # Windows only: no newline translation; 0 (no flag) elsewhere

# (path, size, modification time, digest) of files already verified in this process: a 180 MB model is hashed once,
# not on every call
_verified: set[tuple[Path, int, int, str]] = set()


class DownloadError(PhotoFxError, OSError):
    """A model file could not be downloaded or did not verify; the message says how to get it by hand. An OSError too,
    as a failed transfer or an unwritable folder is."""


def fetch(url: str, sha256: str, target: Path, *, size: int) -> Path:
    """Return `target`, downloading it from `url` first unless it is already there with SHA-256 `sha256`.

    url: where the file is: `https://`, or `http://` to this computer (127.0.0.1, ::1, localhost), or `file://`.
    A redirect may lead only to https or to http on this computer. sha256: its expected SHA-256, lowercase hex.
    size: its exact size in bytes; a server that offers or sends more is cut off at once, so it cannot fill the disk.
    target: where it is kept; missing folders are created. A file at `target` with another digest (an earlier broken
    copy) is replaced. Raises DownloadError, naming `url` and `target` so the file can be fetched by hand, when the
    folder cannot be written, the download fails, the URL or a redirect is not allowed, or what arrived has another
    size or digest; `target` is then left as it was and no temporary file remains.
    """
    target = Path(target)
    if _is_verified(target, sha256):
        return target
    by_hand = f"download {url} by hand and save it as {target}"
    scheme = urllib.parse.urlsplit(url).scheme
    if scheme != "file" and not _secure_or_local(url):
        raise DownloadError(f"only https downloads are allowed from other computers, not {scheme}: {url}; {by_hand}")
    temporary = target.parent / f".{target.name}.{secrets.token_hex(8)}.part"
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
        _clear_dead_downloads(target)
        # created like any new file (0o666 less the umask, so the kept file needs no chmod) and only if it is new
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | _BINARY, _NEW_FILE)
    except OSError as error:  # no permission, a file in the way, a read-only or full disk
        raise DownloadError(f"cannot write to the folder {target.parent} ({error}); {by_hand}") from error
    try:
        digest = hashlib.sha256()
        with os.fdopen(descriptor, "wb") as out:  # closed before the rename below, which Windows requires
            try:
                with urllib.request.build_opener(_SecureRedirects).open(url, timeout=_TIMEOUT) as response:
                    too_large = f"what {url} sends is larger than the expected {size} bytes, so it is not the file"
                    offered = response.headers.get("Content-Length")
                    if offered is not None and int(offered) > size:
                        raise DownloadError(f"{too_large}; {by_hand}")
                    received = 0
                    while chunk := response.read(min(_CHUNK, size + 1 - received)):  # one byte past `size` is enough
                        received += len(chunk)
                        if received > size:
                            raise DownloadError(f"{too_large}; {by_hand}")
                        out.write(chunk)
                        digest.update(chunk)
            except DownloadError:  # already says what went wrong (and is an OSError itself)
                raise
            except (OSError, http.client.HTTPException) as error:  # URLError, timeouts and resets are OSErrors
                raise DownloadError(f"could not download {url} ({error}); {by_hand}") from error
        if received != size:
            stopped = f"the download of {url} stopped after {received} of {size} bytes"
            raise DownloadError(f"{stopped}; try again, or {by_hand}")
        if digest.hexdigest() != sha256:
            raise DownloadError(
                f"the file downloaded from {url} has SHA-256 {digest.hexdigest()}, but {sha256} was expected, so it "
                f"was not saved; {by_hand} (and check that it is the right file)"
            )
        temporary.replace(target)
    except BaseException:
        temporary.unlink(missing_ok=True)
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


def _clear_dead_downloads(target: Path) -> None:
    """Delete the temporary files of earlier downloads of `target` that died without cleaning up (a killed process,
    a power cut). One untouched for a day is dead; a younger one may be another process still downloading."""
    for leftover in target.parent.glob(f".{glob.escape(target.name)}.*.part"):
        try:
            if time.time() - leftover.stat().st_mtime > _DEAD_AFTER:
                leftover.unlink()
        except OSError:  # gone already, or held open by its process (Windows): not ours to force
            continue


def _secure_or_local(url: str) -> bool:
    """https anywhere, or http to this computer."""
    parts = urllib.parse.urlsplit(url)
    return parts.scheme == "https" or (parts.scheme == "http" and parts.hostname in _LOOPBACK)


class _SecureRedirects(urllib.request.HTTPRedirectHandler):
    """Follows a redirect only to https, or to http on this computer: never down to plain http across the network."""

    def redirect_request(
        self,
        req: urllib.request.Request,
        fp: IO[bytes],
        code: int,
        msg: str,
        headers: http.client.HTTPMessage,
        newurl: str,
    ) -> urllib.request.Request | None:
        if not _secure_or_local(newurl):
            raise urllib.error.URLError(f"redirected to {newurl}, which is not https")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


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
