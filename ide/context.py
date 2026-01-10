class IDEContext:
    """Capability surface for providers and commands."""

    def __init__(
        self,
        document,
        events,
        commands,
        editor=None,
        preview=None,
        confirm_callback=None,
        present_text_callback=None,
        request_save_path_callback=None,
        choose_option_callback=None,
        review_text_callback=None,
        present_suggestions_callback=None,
    ):
        self.document = document
        self.events = events
        self.editor = editor
        self.preview = preview
        self.commands = commands
        self._confirm_callback = confirm_callback
        self._present_text_callback = present_text_callback
        self._request_save_path_callback = request_save_path_callback
        self._choose_option_callback = choose_option_callback
        self._review_text_callback = review_text_callback
        self._present_suggestions_callback = present_suggestions_callback

    def has_selection(self):
        if not self.editor:
            return False
        return self.editor.textCursor().hasSelection()

    def get_selection(self):
        if not self.editor:
            return ""
        return self.editor.textCursor().selectedText()

    def insert_text(self, text):
        if not self.editor:
            self.document.set_content(self.document.content + text)
            return
        cursor = self.editor.textCursor()
        cursor.insertText(text)
        self.editor.setTextCursor(cursor)
        self.editor.setFocus()

    def set_content(self, content):
        self.document.set_content(content)

    def load(self, file_path):
        return self.document.load(file_path)

    def save(self):
        return self.document.save()

    def save_as(self, file_path):
        return self.document.save_as(file_path)

    def confirm(self, title):
        if self._confirm_callback:
            return self._confirm_callback(title)
        return True

    def present_text(self, title, content, allow_insert=False):
        if self._present_text_callback:
            return self._present_text_callback(title, content, allow_insert)
        return False

    def request_save_path(self, title, file_filter):
        if self._request_save_path_callback:
            return self._request_save_path_callback(title, file_filter)
        return ""

    def choose_option(self, title, message, options):
        if self._choose_option_callback:
            return self._choose_option_callback(title, message, options)
        return ""

    def review_text(self, title, original, revised, options):
        if self._review_text_callback:
            return self._review_text_callback(title, original, revised, options)
        return ""

    def present_suggestions(self, title, suggestions):
        if self._present_suggestions_callback:
            return self._present_suggestions_callback(title, suggestions)
        return ""
