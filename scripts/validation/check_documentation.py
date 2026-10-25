#!/usr/bin/env python3
"""Check current local document navigation and FR-01 through FR-18 anchors."""
import argparse
import json
import os
from pathlib import Path
import re
import urllib.parse

ROOT = Path(__file__).resolve().parents[2]


def anchors(path):
    text = path.read_text()
    ids = set(re.findall(r'<a\s+(?:id|name)=["\']([^"\']+)["\']', text))
    counts = {}
    fenced = False
    for line in text.splitlines():
        if line.lstrip().startswith(("```", "~~~")):
            fenced = not fenced
        if fenced:
            continue
        heading = re.match(r"^#{1,6}\s+(.+?)(?:\s+#+)?$", line)
        if heading:
            title = re.sub(r"<[^>]*>", "", heading[1]).lower()
            slug = re.sub(r"[^\w\s-]", "", title).replace(" ", "-")
            count = counts.get(slug, 0)
            counts[slug] = count + 1
            ids.add(slug if count == 0 else f"{slug}-{count}")
    return ids


def check(root):
    documents = [root / name for name in ["README.md", "AGENTS.md", "CHANGELOG.md"]]
    for directory in ["docs", "crates", "apps", "config", "deploy", "scripts", "tools", "tests"]:
        for current, directories, files in os.walk(root / directory):
            directories[:] = [name for name in directories if name not in ["evidence", "archive", "target", ".cargo"]]
            documents.extend(Path(current) / name for name in files if name.endswith(".md"))
    issues, links = [], 0
    cache = {}
    for path in sorted(set(documents)):
        text = path.read_text()
        references = re.findall(r"!?\[[^\]]*\]\(([^()\s]+)(?:\s+\"[^\"]*\")?\)", text)
        references += re.findall(r"^\[[^\]]+\]:\s*(\S+)", text, re.M)
        for value in references:
            parsed = urllib.parse.urlsplit(value.strip("<>"))
            if parsed.scheme or parsed.netloc:
                continue
            links += 1
            target = (path.parent / urllib.parse.unquote(parsed.path)).resolve() if parsed.path else path
            if not target.exists():
                issues.append({"document": str(path.relative_to(root)), "link": value, "reason": "missing"})
            elif parsed.fragment and target.suffix == ".md":
                if target not in cache:
                    cache[target] = anchors(target)
                if urllib.parse.unquote(parsed.fragment) not in cache[target]:
                    issues.append({"document": str(path.relative_to(root)), "link": value, "reason": "missing_anchor"})
    requirements = root / "docs/requirements/REQUIREMENTS.md"
    heading_ids = anchors(requirements)
    missing_fr = [f"FR-{index:02}" for index in range(1, 19)
                  if not any(anchor.startswith(f"fr-{index:02}-") for anchor in heading_ids)]
    return {"schema_version": 1, "status": "passed" if not issues and not missing_fr else "failed",
            "documents": len(set(documents)), "links": links, "issues": issues, "missing_fr": missing_fr,
            "scope": "Current navigation only; archived content and frozen evidence bytes are not edited."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.output and args.output.exists():
        parser.error("use a new verification report")
    result = check(args.root.resolve())
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
