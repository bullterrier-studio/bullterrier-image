"""Tests of `photo_fx.matte.download`: the verified, atomic first-use download of a model file.

The network is a real boundary served on the loopback interface: a test-owned HTTP server on an ephemeral port, whose
handler counts the requests it answers. Expected digests are computed here with hashlib from the served bytes."""

import hashlib
import socket
import threading
from collections.abc import Iterator
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

from photo_fx.matte.download import DownloadError, fetch, user_cache_dir

PAYLOAD = b"not really a model, but bytes all the same\n" * 1000


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


@dataclass
class Server:
    """What the loopback server serves, and how often it was asked."""

    url: str
    body: bytes = PAYLOAD
    announce: int | None = None  # Content-Length to claim; more than len(body) cuts the transfer short
    requests: list[str] = field(default_factory=list)


@pytest.fixture
def server() -> Iterator[Server]:
    state = Server(url="")

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            state.requests.append(self.path)
            self.send_response(200)
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


def test_a_verified_download_lands_at_the_target_with_the_served_bytes(server, tmp_path):
    target = tmp_path / "cache" / "model.onnx"  # the folder does not exist yet
    assert fetch(server.url, sha256(PAYLOAD), target) == target
    assert target.read_bytes() == PAYLOAD
    assert files_in(target.parent) == ["model.onnx"]  # no temporary file left beside it


def test_a_second_fetch_uses_the_cached_file_without_asking_the_server(server, tmp_path):
    target = tmp_path / "model.onnx"
    fetch(server.url, sha256(PAYLOAD), target)
    fetch(server.url, sha256(PAYLOAD), target)
    assert len(server.requests) == 1


def test_a_checksum_mismatch_is_rejected_and_nothing_is_cached(server, tmp_path):
    expected = sha256(b"the model we pinned")
    with pytest.raises(DownloadError) as caught:
        fetch(server.url, expected, tmp_path / "model.onnx")
    assert expected in str(caught.value)
    assert sha256(PAYLOAD) in str(caught.value)  # what arrived instead
    assert files_in(tmp_path) == []


def test_an_interrupted_download_leaves_no_partial_file(server, tmp_path):
    server.announce = len(PAYLOAD) * 2  # the connection closes halfway through what was promised
    with pytest.raises(DownloadError, match=r"model\.onnx"):
        fetch(server.url, sha256(PAYLOAD), tmp_path / "model.onnx")
    assert files_in(tmp_path) == []


def test_a_corrupt_cached_file_is_downloaded_again_and_replaced(server, tmp_path):
    target = tmp_path / "model.onnx"
    target.write_bytes(PAYLOAD[:100])  # an earlier, broken copy
    fetch(server.url, sha256(PAYLOAD), target)
    assert target.read_bytes() == PAYLOAD
    assert len(server.requests) == 1


def test_offline_explains_how_to_download_by_hand(tmp_path):
    with socket.socket() as probe:  # a loopback port nobody listens on: the connection is refused at once
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    url = f"http://127.0.0.1:{port}/model.onnx"
    target = tmp_path / "model.onnx"
    with pytest.raises(DownloadError) as caught:
        fetch(url, sha256(PAYLOAD), target)
    message = str(caught.value)
    assert url in message
    assert str(target) in message  # where to save a copy downloaded by hand
    assert files_in(tmp_path) == []


def test_a_file_url_is_fetched_like_any_other(tmp_path):
    source = tmp_path / "source.onnx"
    source.write_bytes(PAYLOAD)
    target = tmp_path / "cache" / "model.onnx"
    fetch(source.as_uri(), sha256(PAYLOAD), target)
    assert target.read_bytes() == PAYLOAD


@pytest.mark.parametrize(
    ("platform", "environ", "expected"),
    [
        ("darwin", {}, Path("/home/ada/Library/Caches/photo-fx")),
        ("linux", {}, Path("/home/ada/.cache/photo-fx")),
        ("linux", {"XDG_CACHE_HOME": "/var/cache/ada"}, Path("/var/cache/ada/photo-fx")),
        ("linux", {"XDG_CACHE_HOME": ""}, Path("/home/ada/.cache/photo-fx")),  # empty means unset (XDG spec)
        ("win32", {"LOCALAPPDATA": "/Users/ada/AppData/Local"}, Path("/Users/ada/AppData/Local/photo-fx/Cache")),
        ("win32", {}, Path("/home/ada/AppData/Local/photo-fx/Cache")),
    ],
)
def test_user_cache_dir_follows_each_platforms_convention(platform, environ, expected):
    assert user_cache_dir(platform, environ, home=Path("/home/ada")) == expected
