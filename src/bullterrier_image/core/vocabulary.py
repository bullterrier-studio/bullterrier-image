"""The shared parameter vocabulary: names effects must not use for a parameter, and the name to use instead.

Effects are written by many hands; without this, the same idea drifts into `strength`, `amount` and `intensity`, and
every recipe author has to look each one up. A contract test checks every registered effect against this table.
"""

DISCOURAGED = {
    "strength": "amount",
    "intensity": "amount",
    "factor": "amount",
    "colour": "color",
    "rotation": "angle",
    "random_state": "seed",
    "rng": "seed",
}
