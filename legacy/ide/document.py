class Document:
    """Single source of truth for document content."""

    def __init__(self, events):
        self._content = ""
        self._last_saved_content = ""
        self.file_path = None
        self.modified = False
        self.events = events

    @property
    def content(self):
        return self._content

    def load(self, file_path):
        try:
            with open(file_path, "r", encoding="utf-8") as handle:
                self._content = handle.read()
            self.file_path = file_path
            self._last_saved_content = self._content
            self.modified = False
            if self.events:
                self.events.document_changed.emit(self)
            return True
        except Exception:
            return False

    def save(self):
        if not self.file_path:
            return False
        try:
            with open(self.file_path, "w", encoding="utf-8") as handle:
                handle.write(self._content)
            self._last_saved_content = self._content
            self.modified = False
            if self.events:
                self.events.document_saved.emit(self)
            return True
        except Exception:
            return False

    def save_as(self, file_path):
        self.file_path = file_path
        return self.save()

    def set_content(self, content):
        if content != self._content:
            self._content = content
            self.modified = True
            if self.events:
                self.events.document_changed.emit(self)

    @property
    def last_saved_content(self):
        return self._last_saved_content
