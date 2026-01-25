from dataclasses import dataclass
from typing import Optional, Callable


@dataclass(frozen=True)
class Command:
    command_id: str
    title: str
    handler: Callable
    requires_selection: bool = False
    modifies_document: bool = False
    confirm: Optional[str] = None


class CommandRegistry:
    def __init__(self):
        self._commands = {}

    def register(self, command):
        if command.command_id in self._commands:
            raise ValueError(f"Command already registered: {command.command_id}")
        self._commands[command.command_id] = command

    def unregister(self, command_id):
        self._commands.pop(command_id, None)

    def get(self, command_id):
        return self._commands.get(command_id)

    def execute(self, command_id, context):
        command = self._commands.get(command_id)
        if not command:
            return False
        if command.requires_selection and not context.has_selection():
            return False
        if command.confirm and not context.confirm(command.confirm):
            return False
        result = command.handler(context)
        return True if result is None else result
