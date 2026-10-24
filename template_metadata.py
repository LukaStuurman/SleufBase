"""Title-block values shared by every sheet of one template export."""
from datetime import datetime


def project_name(value: str) -> str:
    name = str(value).strip()
    if len(name.encode("utf-8")) > 4096 or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in name):
        raise ValueError("Gebruik een projectnaam op één regel van maximaal 4096 UTF-8-bytes.")
    return name


def update_title_blocks(document, layout_names, *, drawn_by="", project=None, export_date=None):
    # Capture once: pages of an export spanning midnight still share one date.
    date = (export_date or datetime.now()).strftime("%d-%m-%Y")
    name = None if project is None else project_name(project)
    for layout_name in layout_names:
        try:
            layout = document.layouts.get(layout_name)
        except KeyError:
            continue
        for insert in layout.query("INSERT"):
            attributes = list(insert.attribs)
            tags = {str(a.dxf.tag or "").strip().upper() for a in attributes}
            values = {}
            for numbered, plain, value in (
                ("DATUM1", "DATUM", date),
                ("OMSCHRIJVING1", "OMSCHRIJVING", "Proefsleuven"),
                ("TEKENAAR1", "TEKENAAR", str(drawn_by or "").strip()),
            ):
                values[numbered if numbered in tags else plain] = value
            seen_plain = set()
            for attribute in attributes:
                tag = str(attribute.dxf.tag or "").strip().upper()
                if name is not None and tag in {"GEM-PROJECT", "PROJECTOMSCHRIJVING", "PROJECTNAAM"}:
                    attribute.dxf.text = name
                elif tag in values and tag not in seen_plain:
                    attribute.dxf.text = values[tag]
                    # Repeated unnumbered tags are later revisions, not today's date.
                    if tag in {"DATUM", "OMSCHRIJVING", "TEKENAAR"}:
                        seen_plain.add(tag)
