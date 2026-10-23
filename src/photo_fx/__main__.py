"""`python -m photo_fx`: the same command line as the `photo-fx` program."""

import sys

from .cli import main

sys.exit(main())
