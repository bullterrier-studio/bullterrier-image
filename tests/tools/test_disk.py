"""tools/disk.py: the incremental build output is deleted when the disk is short of room, and only then."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def _load():
    spec = importlib.util.spec_from_file_location("disk", ROOT / "tools" / "disk.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


disk = _load()


def folder_with_files(tmp_path):
    folder = tmp_path / "incremental"
    (folder / "crate-1").mkdir(parents=True)
    (folder / "crate-1" / "dep-graph.bin").write_bytes(b"x")
    return folder


def test_below_the_threshold_the_folder_is_deleted_and_at_or_above_it_is_left(tmp_path):
    folder = folder_with_files(tmp_path)
    assert disk.prune(free=100, least=100, folder=folder) is False
    assert (folder / "crate-1" / "dep-graph.bin").is_file()
    assert disk.prune(free=99, least=100, folder=folder) is True
    assert not folder.exists()


def test_a_missing_folder_is_not_an_error(tmp_path):
    assert disk.prune(free=0, least=100, folder=tmp_path / "none") is False
