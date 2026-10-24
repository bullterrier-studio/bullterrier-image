# Security policy

## Reporting a vulnerability

Please report security problems privately, not in a public issue: use GitHub's private vulnerability reporting at
<https://github.com/jcakstins/photo-fx/security/advisories/new> ("Report a vulnerability" on the repository's Security
tab). Say what is affected, how to reproduce it (a small file or recipe helps), and what an attacker could do with it.

You can expect an acknowledgement within a week. Once a fix is released, the advisory is published with credit to you,
unless you prefer otherwise.

## Supported versions

bullterrier-image is before 1.0: only the latest release receives security fixes.

| Version | Supported |
| --- | --- |
| latest 0.x release | yes |
| older releases | no |

## What bullterrier-image does to stay safe

- **The subject model download is pinned.** The portable subject finder downloads its model on first use, over HTTPS
  (redirects included) from an address written in the code, and keeps it only if its exact size and SHA-256 match the
  ones written there; anything else is discarded and no partial file is left. Nothing is downloaded when bullterrier-image is
  imported. `BT_IMAGE_MODEL` points at a copy you provide instead.
- **Untrusted pictures.** Pillow's decompression-bomb limit stays on, and a damaged file raises `ImageReadError`.
- **Cached data** is read without pickle, so a planted cache file cannot run code.
- **Outputs** carry no EXIF or GPS data from the input photo.
- **Audited code and dependencies.** ruff's security rules (flake8-bandit) are part of every `poe check`, and
  `uv run poe audit` checks every release pinned in `uv.lock` with pip-audit. No known advisory is open; one that has no
  fixed release yet would be listed here by its id.
- **Plugins** are Python packages: installing one runs its code, as with any package. `BT_IMAGE_PLUGINS=builtin` loads
  only bullterrier-image's own effects.
