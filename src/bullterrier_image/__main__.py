"""`python -m bullterrier_image`: the same command line as the `bt-image` program."""

import sys

from .cli import main

sys.exit(main())
