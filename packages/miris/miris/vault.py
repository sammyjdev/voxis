"""Render the meeting note and write it into the vault."""
import datetime as dt
import json
import os
import re
import unicodedata
from pathlib import Path

from miris.config import Config


def slugify(text: str, max_len: int = 60) -> str:
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    text = re.sub(r"[^a-zA-Z0-9]+", "-", text).strip("-").lower()
    return text[:max_len] or "reuniao"


def render_note(notes: dict, transcript: str, session_id: str, date: dt.date) -> str:
    """Full note body: YAML frontmatter + resumo + action items + transcript."""
    items = "\n".join(f"- [ ] {i}" for i in notes.get("action_items", []))
    # json.dumps = valid YAML scalar quoting (titles often contain ":")
    return f"""---
title: {json.dumps(notes['titulo'], ensure_ascii=False)}
date: {date.isoformat()}
categoria: {json.dumps(notes['categoria'], ensure_ascii=False)}
source: miris
session: {json.dumps(session_id)}
---

# {notes['titulo']}

## Resumo

{notes['resumo']}

## Action items

{items}

## Transcript

{transcript}
"""


def _write_text_atomic(path: Path, text: str) -> None:
    tmp = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, path)


def _has_session(path: Path, session_line: str) -> bool:
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != "---":
        return False
    try:
        end = lines.index("---", 1)
    except ValueError:
        return False
    return session_line in lines[1:end]


def save_note(cfg: Config, notes: dict, transcript: str, session_id: str,
              date: dt.date | None = None) -> Path:
    date = date or dt.date.today()
    folder = cfg.categories.get(notes["categoria"], cfg.inbox)
    directory = cfg.vault_root / folder
    directory.mkdir(parents=True, exist_ok=True)
    base = f"{date.isoformat()}-{slugify(notes['titulo'])}"
    session_line = f"session: {json.dumps(session_id)}"
    path = directory / f"{base}.md"
    n = 2
    while path.exists():
        if _has_session(path, session_line):
            break
        path = directory / f"{base}-{n}.md"
        n += 1
    _write_text_atomic(path, render_note(notes, transcript, session_id, date))
    return path
