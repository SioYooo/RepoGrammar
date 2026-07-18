"""Escape-ratio budget fixture for the sync-equivalence oracle.

The body-local blob string below is grown by the `python_context_budget`
scenario to push the whole-project Python context payload across the
incremental-sync budget without changing this module's interface projection
(top-level symbols, literal ``__all__``, ``__init__`` re-exports). The scenario
fills it with control characters because each one serde_json escapes to a
six-byte unicode-escape sequence, exercising the worst-case JSON-escape
expansion the size-only budget gate must bound with its 6x headroom.
"""


def escape_heavy_blob() -> str:
    return "ESCAPE_HEAVY_BLOB_SENTINEL"
