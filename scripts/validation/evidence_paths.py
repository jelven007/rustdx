"""Resolve retained historical workspace paths without modifying frozen records."""
from pathlib import Path
import json

WORKSPACE = Path(__file__).resolve().parents[2]
_MOVES = (
    ("docs/validation", "docs/testing"),
    ("tests", "crates/rustdx-core/tests"),
    ("examples", "crates/rustdx-core/examples"),
    ("src", "crates/rustdx-core/src"),
    ("rustdx-cmd", "apps/rustdx-cli"),
    ("rustdx-runner", "apps/rustdx-runner"),
)

def relocated_path(root, relative):
    """Compose recorded moves; only return paths confined to the requested workspace."""
    for _ in range(4):
        if (root / relative).exists():
            return root / relative
        previous = relative
        for old, new in _MOVES:
            if relative == old or relative.startswith(old + "/"):
                relative = new + relative[len(old):]
                break
        mapping = root / "docs/testing/evidence/production-lifecycle-20261025/migration.json"
        if mapping.is_file():
            records = json.loads(mapping.read_text())["files"]
            relative = next((row["new"] for row in records if row["old"] == relative), relative)
        destination = (root / relative).resolve()
        destination.relative_to(root.resolve())
        if previous == relative:
            break
    return root / relative


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
    return relocated_path(WORKSPACE, relative)
