# document_events.py
from PyQt6.QtCore import QObject, pyqtSignal

class DocumentEvents(QObject):
    document_changed = pyqtSignal(object)   # Document
    document_saved = pyqtSignal(object)
    cursor_moved = pyqtSignal(int)
    document_opened = pyqtSignal(object)
    document_closed = pyqtSignal(object)
