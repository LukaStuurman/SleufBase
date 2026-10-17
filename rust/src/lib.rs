//! SleufBase Rust port — local KickTheMap data model and exports.
//! The historical Python application remains unchanged until parity is reached.

pub mod disciplines;
pub mod geo;
pub mod geotiff;
pub mod loaded_jobs;
pub mod raster;
pub mod raster_loader;
mod raster_color;
mod tiff_view;
pub mod dxf_read;
mod dxf_tags;
pub mod project;
pub mod legacy_project;
pub mod storage;
pub mod virtual_trench;
pub mod cross_section;
pub mod pdok_wms;
pub mod jobs_cache;
pub mod kickthemap_live;
pub mod bgt;
pub mod ahn;
pub mod marxact;
pub mod marxact_alignment;
pub mod cadastral;
pub mod settings_compat;
pub mod export;
pub mod model;
pub mod parse;

pub use disciplines::{summarize, matching_rule, Rule, Summary, RULES};
pub use export::{write_dxf, write_dxf_with_trenches, write_dxf_with_layers, write_xlsx};
pub use model::{Bounds, Dataset, Feature, Geometry, Vertex};
pub use parse::{load_job_features, parse_job_features};
