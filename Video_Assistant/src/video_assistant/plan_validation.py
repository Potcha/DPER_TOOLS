"""Portable Edit Plan validation shared before any adapter invocation."""

from __future__ import annotations

from .edit_plan import EditDocument, EditOperation, OperationKind
from .errors import ValidationError


def validate_edit_document(document: EditDocument) -> tuple[ValidationError, ...]:
    errors: list[ValidationError] = []
    operation_ids: set[str] = set()
    created_ids: set[str] = set()
    for index, operation in enumerate(document.operations):
        path = f"$.operations[{index}]"
        if operation.id in operation_ids:
            errors.append(ValidationError("operation_id_duplicate", f"{path}.id", "operation id must be unique"))
        operation_ids.add(operation.id)
        if operation.kind in {OperationKind.INSERT_CLIP, OperationKind.ADD_MUSIC}:
            created_ids.add(operation.id)
        _validate_operation(operation, path, created_ids, errors)
    return tuple(errors)


def _validate_operation(
    operation: EditOperation,
    path: str,
    created_ids: set[str],
    errors: list[ValidationError],
) -> None:
    params = operation.params
    reference_kinds = {OperationKind.TRIM_CLIP: "operation_id", OperationKind.SET_VOLUME: "operation_id"}
    if operation.kind in reference_kinds:
        reference = params.get(reference_kinds[operation.kind])
        if reference not in created_ids:
            errors.append(
                ValidationError(
                    "operation_reference",
                    f"{path}.params.operation_id",
                    f"must reference an earlier media operation, got {reference!r}",
                )
            )
    for key in ("record_frame", "duration_frames", "source_in_frames", "source_out_frames"):
        if key in params:
            value = params[key]
            if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
                errors.append(ValidationError("frame_value", f"{path}.params.{key}", "expected a non-negative number"))
    if operation.kind == OperationKind.SET_VOLUME and "level_db" in params:
        level = params["level_db"]
        if isinstance(level, bool) or not isinstance(level, (int, float)) or not -100 <= level <= 30:
            errors.append(ValidationError("volume_range", f"{path}.params.level_db", "Resolve range is -100 to 30 dB"))

