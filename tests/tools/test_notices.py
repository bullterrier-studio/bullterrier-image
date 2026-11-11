"""tools/notices.py: THIRD-PARTY-NOTICES.md fails the gate for a licence block with no text."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def _load():
    spec = importlib.util.spec_from_file_location("notices", ROOT / "tools" / "notices.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


notices = _load()
LICENCE = "Permission is hereby granted, free of charge, to any person obtaining a copy of this software. " * 3


def block(name: str, text: str) -> str:
    return f"## {name}\n\nUsed by:\n\n- a-crate 1.0.0 (https://example.org)\n\n```text\n{text}\n```\n\n"


def test_a_licence_block_with_no_text_or_a_stub_is_named_and_a_full_one_is_not():
    text = block("MIT License", LICENCE) + block("Odd License", "") + block("Short License", "see the crate")
    assert notices.empty_licences(text) == ["Odd License", "Short License"]
    assert notices.empty_licences(block("MIT License", LICENCE)) == []


def test_the_notices_of_this_checkout_have_a_text_for_every_licence_and_name_no_crate_of_our_own():
    text = (ROOT / "THIRD-PARTY-NOTICES.md").read_text(encoding="utf-8")
    assert notices.empty_licences(text) == []
    assert text.count("```text") >= 5  # MIT, Apache-2.0, ISC, Unicode-3.0, Zlib at least
    assert "- bullterrier-image" not in text
