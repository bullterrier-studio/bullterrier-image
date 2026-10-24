"""Tests of `bullterrier_image.matte.download`: the verified, atomic first-use download of a model file.

The network is a real boundary served on the loopback interface: a test-owned HTTP server on an ephemeral port, whose
handler counts the requests it answers. Expected digests are computed here with hashlib from the served bytes."""

import hashlib
import os
import re
import socket
import stat
import sys
import threading
import time
from collections.abc import Iterator
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

from bullterrier_image import BullterrierImageError
from bullterrier_image.matte.download import DownloadError, fetch, user_cache_dir

PAYLOAD = b"not really a model, but bytes all the same\n" * 1000


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


@dataclass
class Server:
    """What the loopback server serves, and how often it was asked."""

    url: str
    body: bytes = PAYLOAD
    announce: int | None = None  # Content-Length to claim; more than len(body) cuts the transfer short
    length_header: bool = True  # False: no Content-Length, the end of the body is where the connection closes
    redirects: dict[str, str] = field(default_factory=dict)  # request path -> where it redirects to
    requests: list[str] = field(default_factory=list)


@pytest.fixture
def server() -> Iterator[Server]:
    state = Server(url="")

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            state.requests.append(self.path)
            if self.path in state.redirects:
                self.send_response(302)
                self.send_header("Location", state.redirects[self.path])
                self.send_header("Content-Length", "0")
                self.end_headers()
                return
            self.send_response(200)
            if state.length_header:
                self.send_header("Content-Length", str(state.announce or len(state.body)))
            self.end_headers()
            self.wfile.write(state.body)
            self.close_connection = True

        def log_message(self, *args):  # keep the test output clean
            pass

    httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=httpd.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
    thread.start()
    state.url = f"http://127.0.0.1:{httpd.server_address[1]}/model.onnx"
    try:
        yield state
    finally:
        httpd.shutdown()
        httpd.server_close()
        thread.join()


def files_in(folder: Path) -> list[str]:
    return sorted(p.name for p in folder.iterdir()) if folder.exists() else []


def test_a_failed_download_is_one_of_bullterrier_images_own_errors(tmp_path):
    with pytest.raises(BullterrierImageError, match="only https"):
        fetch("ftp://models.example.invalid/model.onnx", sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))


def test_a_verified_download_lands_at_the_target_with_the_served_bytes(server, tmp_path):
    target = tmp_path / "cache" / "model.onnx"  # the folder does not exist yet
    assert fetch(server.url, sha256(PAYLOAD), target, size=len(PAYLOAD)) == target
    assert target.read_bytes() == PAYLOAD
    assert files_in(target.parent) == ["model.onnx"]  # no temporary file left beside it


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX permission bits; Windows has no group or other bits")
def test_the_kept_file_is_readable_like_any_file_the_user_saves(server, tmp_path):
    """Not only by its owner, as a private temporary file would be: the usual 0o666 less the user's umask (an
    unusual umask here, so a fixed mode would show)."""
    umask = os.umask(0o027)
    try:
        target = fetch(server.url, sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))
    finally:
        os.umask(umask)
    assert stat.S_IMODE(target.stat().st_mode) == 0o640


def test_a_second_fetch_uses_the_cached_file_without_asking_the_server(server, tmp_path):
    target = tmp_path / "model.onnx"
    fetch(server.url, sha256(PAYLOAD), target, size=len(PAYLOAD))
    fetch(server.url, sha256(PAYLOAD), target, size=len(PAYLOAD))
    assert len(server.requests) == 1


def test_a_checksum_mismatch_is_rejected_and_nothing_is_cached(server, tmp_path):
    expected = sha256(b"the model we pinned")
    with pytest.raises(DownloadError) as caught:
        fetch(server.url, expected, tmp_path / "model.onnx", size=len(PAYLOAD))
    assert expected in str(caught.value)
    assert sha256(PAYLOAD) in str(caught.value)  # what arrived instead
    assert files_in(tmp_path) == []


def test_an_interrupted_download_leaves_no_partial_file(server, tmp_path):
    server.announce = len(PAYLOAD) * 2  # the connection closes halfway through what was promised
    with pytest.raises(DownloadError, match=r"model\.onnx"):
        fetch(server.url, sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))
    assert files_in(tmp_path) == []


@pytest.mark.parametrize("length_header", [True, False])
def test_a_download_larger_than_the_file_asked_for_is_stopped_and_nothing_is_kept(server, tmp_path, length_header):
    """Announced or not, more bytes than the pinned size means it is not the file: stop before the disk fills."""
    server.body = PAYLOAD * 3
    server.length_header = length_header
    with pytest.raises(DownloadError, match=f"larger than the expected {len(PAYLOAD)} bytes") as caught:
        fetch(server.url, sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))
    assert str(caught.value).count("by hand") == 1  # said once, not wrapped in a second "could not download"
    assert files_in(tmp_path) == []


def test_a_download_that_ends_early_says_how_far_it_got(server, tmp_path):
    server.body = PAYLOAD[:1000]
    server.length_header = False  # nothing announced, so only the pinned size shows that bytes are missing
    with pytest.raises(DownloadError, match=f"stopped after 1000 of {len(PAYLOAD)} bytes"):
        fetch(server.url, sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))
    assert files_in(tmp_path) == []


def test_leftovers_of_a_download_killed_long_ago_are_cleared_by_the_next_download(server, tmp_path):
    """A killed process cannot clean up after itself; the next download does, but only what is clearly dead."""
    two_days_ago = time.time() - 2 * 24 * 3600
    for name in (".model.onnx.dead1.part", ".model.onnx.dead2.part"):
        (tmp_path / name).write_bytes(b"half a model")
        os.utime(tmp_path / name, (two_days_ago, two_days_ago))
    (tmp_path / ".model.onnx.live.part").write_bytes(b"another process, still downloading")  # touched just now
    (tmp_path / ".other.onnx.dead.part").write_bytes(b"not ours")
    os.utime(tmp_path / ".other.onnx.dead.part", (two_days_ago, two_days_ago))
    fetch(server.url, sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))
    assert files_in(tmp_path) == [".model.onnx.live.part", ".other.onnx.dead.part", "model.onnx"]


def test_a_cache_folder_that_cannot_be_written_gives_the_same_advice_before_any_download(server, tmp_path):
    blocker = tmp_path / "not-a-folder"
    blocker.write_text("a file where the cache folder should be", encoding="utf-8")
    target = blocker / "bullterrier-image" / "model.onnx"
    with pytest.raises(DownloadError, match="by hand") as caught:
        fetch(server.url, sha256(PAYLOAD), target, size=len(PAYLOAD))
    assert str(blocker / "bullterrier-image") in str(caught.value)
    assert server.requests == []


def test_a_corrupt_cached_file_is_downloaded_again_and_replaced(server, tmp_path):
    target = tmp_path / "model.onnx"
    target.write_bytes(PAYLOAD[:100])  # an earlier, broken copy
    fetch(server.url, sha256(PAYLOAD), target, size=len(PAYLOAD))
    assert target.read_bytes() == PAYLOAD
    assert len(server.requests) == 1


def test_offline_explains_how_to_download_by_hand(tmp_path):
    with socket.socket() as probe:  # a loopback port nobody listens on: the connection is refused at once
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    url = f"http://127.0.0.1:{port}/model.onnx"
    target = tmp_path / "model.onnx"
    with pytest.raises(DownloadError) as caught:
        fetch(url, sha256(PAYLOAD), target, size=len(PAYLOAD))
    message = str(caught.value)
    assert url in message
    assert str(target) in message  # where to save a copy downloaded by hand
    assert files_in(tmp_path) == []


@pytest.mark.parametrize(
    "url",
    [
        # .invalid never resolves (RFC 2606), so not even a refused check could reach a real host
        "http://models.example.invalid/model.onnx",  # plain http to another computer: no TLS
        "ftp://models.example.invalid/model.onnx",
        "data:application/octet-stream;base64,AAAA",
    ],
)
def test_a_url_to_another_computer_must_be_secure(tmp_path, url):
    with pytest.raises(DownloadError, match="only https") as caught:
        fetch(url, sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))
    assert url.split(":")[0] in str(caught.value)  # names the scheme it refused
    assert files_in(tmp_path) == []


@pytest.mark.parametrize("host", ["localhost", "[::1]"])
def test_plain_http_to_this_computer_is_allowed(tmp_path, host):
    """Loopback names other than 127.0.0.1 pass the check; nothing listens there, so the download itself fails."""
    with pytest.raises(DownloadError) as caught:
        fetch(f"http://{host}:9/model.onnx", sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))
    assert "only https" not in str(caught.value)


def test_a_redirect_away_from_https_is_refused(server, tmp_path):
    server.redirects["/model.onnx"] = "http://example.invalid/model.onnx"
    with pytest.raises(DownloadError, match=re.escape("redirected to http://example.invalid")):
        fetch(server.url, sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))
    assert files_in(tmp_path) == []


def test_a_redirect_that_stays_on_this_computer_is_followed(server, tmp_path):
    server.redirects["/model.onnx"] = "/moved/model.onnx"
    target = fetch(server.url, sha256(PAYLOAD), tmp_path / "model.onnx", size=len(PAYLOAD))
    assert target.read_bytes() == PAYLOAD
    assert server.requests == ["/model.onnx", "/moved/model.onnx"]


def test_a_file_url_is_fetched_like_any_other(tmp_path):
    source = tmp_path / "source.onnx"
    source.write_bytes(PAYLOAD)
    target = tmp_path / "cache" / "model.onnx"
    fetch(source.as_uri(), sha256(PAYLOAD), target, size=len(PAYLOAD))
    assert target.read_bytes() == PAYLOAD


@pytest.mark.parametrize(
    ("platform", "environ", "expected"),
    [
        ("darwin", {}, Path("/home/ada/Library/Caches/bt-image")),
        ("linux", {}, Path("/home/ada/.cache/bt-image")),
        ("linux", {"XDG_CACHE_HOME": "/var/cache/ada"}, Path("/var/cache/ada/bt-image")),
        ("linux", {"XDG_CACHE_HOME": ""}, Path("/home/ada/.cache/bt-image")),  # empty means unset (XDG spec)
        ("win32", {"LOCALAPPDATA": "/Users/ada/AppData/Local"}, Path("/Users/ada/AppData/Local/bt-image/Cache")),
        ("win32", {}, Path("/home/ada/AppData/Local/bt-image/Cache")),
    ],
)
def test_user_cache_dir_follows_each_platforms_convention(platform, environ, expected):
    assert user_cache_dir(platform, environ, home=Path("/home/ada")) == expected
