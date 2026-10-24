The requested project/date feature is characterized with the shipped template
and a deterministic ezdxf 1.4.4 drawing containing two title inserts per sheet,
active/inactive paper layouts, unused sheets, model-space inserts, ATTDEFs,
numbered revisions and repeated unnumbered revision tags. The project field is
`GEM-PROJECT`, `PROJECTOMSCHRIJVING` or `PROJECTNAAM`; project numbers and drawing
descriptions are preserved. Only the first revision gets the export date.

`tools/generate_template_metadata_fixtures.py` computes expected values through
the Python title-block helper. The native audit reads every source/output DXF
tag with Python and verifies exact preservation outside the intended ATTRIB text
fields. Its fixed date is 18-10-2026; live exports use the local Windows calendar,
captured once per document using [GetLocalTime](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-getlocaltime).
The audit also compares that calendar with Python. The native edit batch has a
32 MiB metadata budget and a 10,000-attribute limit per insert, and checks
cancellation during each source scan before committing any changes.

The desktop Python template-export window already accepts the project name.
Native title editing is available as a Rust kernel; connection to the complete
native template-export window remains part of the unfinished migration.
