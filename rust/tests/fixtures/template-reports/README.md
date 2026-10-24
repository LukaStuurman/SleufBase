# Automatic template discipline workbooks

The generator invokes the actual Python `template_export_plan`,
`summarize_template_export_plan` and `write_discipline_workbook` without opening
an app/account. Ten original rule sets produce 60 report rows: reordered and
duplicate PS variants, a blank DXF slot, unavailable/failed measurement sources,
Unicode labels and literal formula-looking text. The existing documented fix
for casefold-equivalent column names is retained. Two distinct Python hash seeds
reproduce the fixture bytes.

`export_template_reports` calls the production Rust workbook writer. Independent
ZIP/XML readback compares all cells/counts/totals, fonts/fills/alignment, widths,
row heights, panes and filters with the original workbooks. The five production
template-worker exports additionally compare their automatic XLSX files with
the original workflow. A real CLI export opts in through
`settings.discipline_excel: true` and checks its companion alongside DXF readback.

Desktop exports follow the existing automatic discipline-Excel preference.
Counting is independent of profile drawing: blank/excluded slots are omitted,
unavailable data yields a zero row and a notice, names/variants remain in export
order, and measurements stay borrowed. A cache retains at most 2 MiB of count
buffers and 4,096 source entries, avoiding repeated classification of variants.
This is a component budget, not a whole-workbook memory cap or app benchmark.

The DXF is published first. Workbook publication is atomic; replacement keeps
the previous file as `previous-disciplines.xlsx` in the new durable export bundle
(bounded to 128 MiB, with flush/sync). Loaded sources, manifest and DXF aliases
are protected. Directories/symlinks are refused. Locked-output, failed backup,
missing rules and report panic preserve the successful DXF and previous report
and produce a separate notice. This is two publications, not an atomic pair;
cancellation is closed once the DXF crosses its publication boundary.

Full real-project acceptance, paired normal/reverse DXF output, complete Python
project/settings compatibility and the other PARITY.md release gates remain open.
