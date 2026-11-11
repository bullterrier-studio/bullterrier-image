"""The command line at the process level: it is the Rust core's (`crates/cli`, tested in-process there), so what is
tested here is what only Python adds: the entry points, the version, and the effects written in Python."""

import os
import subprocess
import sys
import tomllib
from pathlib import Path

import pytest

import bullterrier_image
from bullterrier_image import imageio
from bullterrier_image.cli import run

ROOT = Path(__file__).resolve().parents[1]


def bt(*args: str, cwd: Path = ROOT) -> subprocess.CompletedProcess[str]:
    """`python -m bullterrier_image ARGS` in a fresh interpreter, the way a user starts it."""
    environment = {key: value for key, value in os.environ.items() if key != "BT_IMAGE_DEBUG"}
    return subprocess.run(
        [sys.executable, "-m", "bullterrier_image", *args],
        cwd=cwd, env=environment, capture_output=True, encoding="utf-8", check=False, timeout=120,
    )  # fmt: skip


def test_version_is_the_one_in_pyproject():
    declared = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
    done = bt("--version")
    assert (done.returncode, done.stdout.strip()) == (0, f"bt-image {declared}")
    assert bullterrier_image.__version__ == declared


def test_the_help_lists_the_commands_and_the_exit_codes():
    done = bt("--help")
    assert done.returncode == 0
    for word in ("render", "sheet", "sweep", "cutout", "run", "steps", "init", "effects", "130"):
        assert word in done.stdout, word


def test_a_render_through_python_dash_m_writes_its_file_and_prints_it(tmp_path, photo):
    imageio.save(photo, tmp_path / "me.png")
    done = bt("render", "me.png", "glitch", "-o", "out", cwd=tmp_path)
    assert done.returncode == 0, done.stderr
    assert done.stdout.split() == [str(Path("out") / "me__glitch.png")]
    assert (tmp_path / "out" / "me__glitch.png").is_file()


def test_a_user_mistake_is_one_error_line_with_exit_1_and_no_traceback(tmp_path):
    done = bt("render", "gone.png", "glitch", cwd=tmp_path)
    assert done.returncode == 1
    assert done.stderr.splitlines()[-1].startswith("error: ")
    assert "Traceback" not in done.stderr


def test_a_wrong_command_line_is_usage_error_2():
    done = bt("frobnicate")
    assert done.returncode == 2
    assert "Traceback" not in done.stderr


def test_an_effect_written_in_python_is_known_to_the_command_line(
    tmp_path, photo, register_probe, throwaway_effects, capfd
):
    register_probe("zz_plugin", "light")
    assert run(["effects", "--category", "light"]) == 0
    assert "  zz_plugin: " in capfd.readouterr().out
    assert run(["effects", "zz_plugin"]) == 0
    assert "help(fx.zz_plugin)" in capfd.readouterr().out
    imageio.save(photo, tmp_path / "me.png")
    (tmp_path / "r.toml").write_text('name = "r"\nsize = 32\n[[step]]\neffect = "zz_plugin"\n', encoding="utf-8")
    assert run(["render", str(tmp_path / "me.png"), str(tmp_path / "r.toml"), "-o", str(tmp_path / "out")]) == 0
    assert (tmp_path / "out" / "me__r.png").is_file()


def test_the_console_entry_point_restores_the_default_signal_handler_and_answers(tmp_path):
    code = "import sys\nfrom bullterrier_image import cli\nsys.argv = ['bt-image', '--version']\ncli.main()\n"
    done = subprocess.run([sys.executable, "-c", code], cwd=tmp_path, capture_output=True, text=True, check=False)
    assert done.returncode == 0, done.stderr
    assert done.stdout.startswith("bt-image ")


@pytest.mark.parametrize("command", ["gallery", "regenerate", "new-effect"])
def test_developer_commands_are_not_part_of_the_installed_cli(command):
    assert bt(command).returncode == 2
