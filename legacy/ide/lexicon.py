import json
from pathlib import Path


class LocalLexicon:
    """Local definitions + synonyms dictionary."""

    def __init__(self, path=None, extra_paths=None):
        if path is None:
            base_dir = Path(__file__).resolve().parent
            path = base_dir / "data" / "lexicon.json"
        self.path = Path(path)
        self.extra_paths = extra_paths or []
        self._entries = {}
        self._load()

    def _load(self):
        if not self.path.exists():
            self._entries = {}
            return
        try:
            with self.path.open("r", encoding="utf-8") as handle:
                self._entries = json.load(handle)
        except Exception:
            self._entries = {}
        self._load_extra()

    def _load_extra(self):
        for raw_path in self.extra_paths:
            for path in self._expand_path(raw_path):
                if not path.exists():
                    continue
                try:
                    with path.open("r", encoding="utf-8") as handle:
                        entries = json.load(handle)
                    if isinstance(entries, dict):
                        self._entries.update(entries)
                except Exception:
                    continue

    def _expand_path(self, raw_path):
        raw = Path(raw_path)
        if "*" in str(raw) or "?" in str(raw):
            return raw.parent.glob(raw.name)
        return [raw]

    def lookup(self, term):
        if not term:
            return None
        key = term.strip().lower()
        return self._entries.get(key)
