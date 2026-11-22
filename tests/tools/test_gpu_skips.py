"""tools/gpu_skips.py: a run of the GPU tests says how many skipped, loudly when any did, and keeps its status."""

import importlib.util
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def _load():
    spec = importlib.util.spec_from_file_location("gpu_skips", ROOT / "tools" / "gpu_skips.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


gpu_skips = _load()

# a stand-in for the test binary: two tests skip, one of them writing its name twice, then the run ends with `status`
SKIPPING = (
    "import os, sys\n"
    "with open(os.environ['BT_GPU_SKIPS'], 'a', encoding='utf-8') as f: f.write('gpu::a\\ngpu::b\\ngpu::a\\n')\n"
    "sys.exit(int(sys.argv[1]))\n"
)


def test_no_skip_is_one_quiet_line_and_skips_are_a_banner_counting_distinct_tests():
    assert gpu_skips.report([]) == "GPU tests skipped: 0"
    banner = gpu_skips.report(["gpu::a", "gpu::b", "gpu::a"])
    assert "2 GPU tests SKIPPED" in banner
    assert banner.startswith("!!!")


def test_the_command_is_told_where_to_write_and_its_status_is_kept(capsys):
    for status in (0, 3):
        assert gpu_skips.main(["--", sys.executable, "-c", SKIPPING, str(status)]) == status
        assert "2 GPU tests SKIPPED" in capsys.readouterr().out


def test_a_command_that_skips_nothing_reports_zero(capsys):
    assert gpu_skips.main(["--", sys.executable, "-c", "pass"]) == 0
    assert "GPU tests skipped: 0" in capsys.readouterr().out


def test_no_command_is_a_usage_error():
    assert gpu_skips.main([]) == 2
    assert gpu_skips.main(["--"]) == 2
