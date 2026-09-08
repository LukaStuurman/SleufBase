from __future__ import annotations

from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class TemplateBlockFootprint:
    name: str
    entity_count: int
    insert_count: int
    image_count: int


@dataclass(frozen=True)
class TemplateFootprint:
    path: str
    file_size_bytes: int
    modelspace_entity_count: int
    block_count: int
    block_entity_count: int
    image_definition_count: int
    largest_blocks: tuple[TemplateBlockFootprint, ...]

    def as_dict(self) -> dict[str, Any]:
        return asdict(self)


def analyze_template_footprint(
    path: str | Path,
    *,
    largest_block_limit: int = 20,
) -> TemplateFootprint:
    """Measure the template without mutating/purging AutoCAD-owned metadata.

    The SleufBase template contains native Dynamic Block donor objects and proxy
    resources. Automatic purge/cleanup is deliberately not performed here: this
    report identifies large blocks for an AutoCAD-validated slimming pass while
    keeping the production template byte-for-byte untouched.
    """

    import ezdxf

    template_path = Path(path)
    document = ezdxf.readfile(template_path)
    modelspace_entity_count = sum(1 for _entity in document.modelspace())
    blocks: list[TemplateBlockFootprint] = []
    block_entity_count = 0
    for block in document.blocks:
        entity_count = 0
        insert_count = 0
        image_count = 0
        for entity in block:
            entity_count += 1
            entity_type = entity.dxftype()
            if entity_type == "INSERT":
                insert_count += 1
            elif entity_type == "IMAGE":
                image_count += 1
        block_entity_count += entity_count
        blocks.append(
            TemplateBlockFootprint(
                name=str(block.name),
                entity_count=entity_count,
                insert_count=insert_count,
                image_count=image_count,
            )
        )

    try:
        image_dict = document.rootdict.get_required_dict("ACAD_IMAGE_DICT")
        image_definition_count = len(image_dict)
    except Exception:
        image_definition_count = 0

    largest = tuple(
        sorted(
            blocks,
            key=lambda item: (-item.entity_count, item.name.casefold()),
        )[: max(1, int(largest_block_limit))]
    )
    return TemplateFootprint(
        path=str(template_path),
        file_size_bytes=int(template_path.stat().st_size),
        modelspace_entity_count=modelspace_entity_count,
        block_count=len(blocks),
        block_entity_count=block_entity_count,
        image_definition_count=image_definition_count,
        largest_blocks=largest,
    )
