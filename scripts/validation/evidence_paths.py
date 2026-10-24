"""Resolve retained historical workspace paths without modifying frozen records."""
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[2]
_MOVES = (
    ("docs/validation", "docs/testing"),
    ("tests", "crates/rustdx-core/tests"),
    ("examples", "crates/rustdx-core/examples"),
    ("src", "crates/rustdx-core/src"),
    ("rustdx-cmd", "apps/rustdx-cli"),
    ("rustdx-runner", "apps/rustdx-runner"),
)


def workspace_path(value):
    """Existing paths win; only paths belonging to this workspace are relocated."""
    path = Path(value)
    if path.is_absolute():
        if path.exists():
            return path
        try:
            relative = path.relative_to(WORKSPACE).as_posix()
        except ValueError:
            return path
    else:
        relative = path.as_posix()
        path = WORKSPACE / path
        if path.exists():
            return path
    for old, new in _MOVES:
        if relative == old or relative.startswith(old + "/"):
            return WORKSPACE / (new + relative[len(old):])
    return path
