# Native sheet extension and layout copies

The offline ezdxf 1.4.4 generator runs the isolated production helpers. Five
sequences contain twelve extension states: eleven successful persisted states
and an atomic failure when no source paper layout exists. The shipped CAD
template expands from 32 to 64 slots; synthetic templates cover a single page,
multiple shifted pages, a pre-existing target layout and repeated no-op calls.

Successful states also run the original/native title-block metadata and viewport
configuration stages. Geometry, ordering, all nondefault DXF attributes, linked
attributes/vertices, nested dictionaries, images, column metadata, dates, project
names, layout properties and viewport clipping links are independently read back
and compared. Existing explicit default tags are semantically equivalent to the
defaults omitted by ezdxf serialization. Numeric tolerance is 1e-9.

Additional witnesses exercise nondefault OCS, MTEXT rotation, bulges/elevation,
embedded R2018 MTEXT insertion versus direction, app data and translated XDATA
points versus unchanged XDATA displacement vectors. The verifier separately
checks persisted embedded positions and preservation of unrelated original tags,
including the original block definitions and unsupported entities not copied.

The source generator proves an original copy defect: nested soft-owned dictionary
entries retain their old owner, and ezdxf audit removes their keys. Native copies
create independent entry objects by actual ownership while keeping the dictionary
flag. Their audit adds no fixes to the original template baseline. Named block,
style and image-definition resources remain shared. Each copied image gets its
own reactor; old reactor groups are not copied. Newly copied linked entities
have valid owners; the shipped source's existing layout-owned SEQEND is retained.

The operation preserves the caller's entire document until success, including
staged text/layers/geometry. Native tests cancel and simulate a worker panic after
a page has already been copied, check exact rollback bytes, and then retry.
Input/output/record/owned-graph budgets, cancellation checks and nonprogress
guards remain active. Progress is reported before committing the final document.

This is the page-extension stage. Legend/title restoration, raster/photo
preparation and the complete native export window still need integration.
DIMENSION/proxy/table/underlay copies and extension object types beyond
DICTIONARY, XRECORD, DICTIONARYVAR and ACDBDICTIONARYWDFLT are explicitly rejected
before the caller is changed; those are remaining migration requirements.
Binary DXF structural editing is still outside the existing preserving editor.
