# Configured rules and discipline workbook parity

`workbooks.json` contains ten independent references produced by the original
Python `build_object_layer_rules`, `summarize_dataset`, `discipline_columns` and
`write_discipline_workbook` functions. The original application is not started.
Default/custom rules, separator parsing, color conversion, empty rules,
fallback labels, overlapping/duplicate labels, Unicode case behavior and literal
formula-like text are represented. Two synthetic jobs are used per case.

One intentional correction is recorded: Python deduplicates column names by
`casefold` but stores counts by exact spelling, dropping later spelling variants
from cells and totals. The corrected oracle combines those counts under the
first column spelling before calling the original workbook writer. Other
values, ordering and formatting come directly from the original writer.

`export_discipline_fixtures` saves native settings and JSON/sidecars, then invokes
the **actual native CLI** for all ten XLSX reports and ten DXF drawings. The
independent Python verifier reads ZIP/XML directly and checks literal cell
types/values, resolved fonts/fills/alignment, row heights, raw column widths,
sheet dimensions, freeze panes, autofilter and totals. It also checks configured
DXF entity layers/colors against the original rule decisions with ezdxf 1.4.4;
all drawings must audit without errors or repairs. ZIP serialization and extra
application metadata need not be byte-identical.

Reproduce using Python 3.11 and the existing development ezdxf dependency:

    python tools/generate_python_case_tables.py
    python tools/generate_discipline_fixtures.py
    cargo test --manifest-path rust/Cargo.toml --test discipline_settings
    cargo build --release --manifest-path rust/Cargo.toml
    cargo run --manifest-path rust/Cargo.toml --example export_discipline_fixtures -- FRESH_OUTPUT_DIRECTORY rust/target/release/sleufbase-rust.exe
    python tools/verify_native_discipline_exports.py FRESH_OUTPUT_DIRECTORY

Case mapping tables pin both uppercase matching and casefold column grouping to
Python 3.11 / Unicode 14, including characters newer Rust versions now classify
differently. The normal ASCII matching path still avoids temporary normalization.
Settings regressions cover old native defaults, explicit empty rules, allowlisted
legacy import and preservation of the previous file on invalid edits. Egui
regressions cover draft isolation and an actually painted configured color.

This verifies the standalone report/configuration path. It does not implement
automatic Excel generation during the still-unported template workflow, all
other old preferences, template-specific source ordering, external account
acceptance or a native Excel application's rendering/print layout.

`application-settings.json` comes from the original validated AppSettings class,
without calling load_settings or reading user files. It records all thirteen app
rules (including heat/high voltage and the monitoring profile label) and twenty
label-gap conversions. The app defaults to 3.5 metres; the bare exporter defaults
to 6 metres. Existing explicit native rules stay unchanged. The independent DXF
checks also compare actual CLI label positions for both default and custom gap.
Regenerate this contract with `python tools/generate_application_settings_fixture.py`.
