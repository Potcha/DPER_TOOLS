"""Shared error model with actionable user-facing messages."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class ValidationError:
    code: str
    path: str
    message: str
    hint: str | None = None

    def __str__(self) -> str:
        location = f"{self.path}: " if self.path else ""
        suffix = f" Hint: {self.hint}" if self.hint else ""
        return f"{location}{self.message} [{self.code}].{suffix}"


class VideoAssistantError(Exception):
    """Base class for expected application failures."""


class DocumentValidationError(VideoAssistantError):
    def __init__(self, errors: tuple[ValidationError, ...]) -> None:
        self.errors = errors
        super().__init__("\n".join(str(error) for error in errors))


class AdapterError(VideoAssistantError):
    """An NLE operation failed after its safety checks."""


class CompilationError(VideoAssistantError):
    """A storyboard cannot be compiled without violating its constraints."""


class MediaIntelligenceError(VideoAssistantError):
    """A deterministic media analysis step failed with a user-facing reason."""

