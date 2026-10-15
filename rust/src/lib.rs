//! SleufBase Rust port — local KickTheMap data model and exports.
//! The historical Python application remains unchanged until parity is reached.

pub mod disciplines;
pub mod geo;
pub mod geotiff;
pub mod loaded_jobs;
pub mod raster;
pub mod dxf_read;
pub mod project;
pub mod virtual_trench;
pub mod pdok_wms;
pub mod jobs_cache;
pub mod kickthemap_live;
pub mod bgt;
pub mod ahn;
pub mod settings_compat;
pub mod export;
pub mod model;
pub mod parse;

pub use disciplines::{summarize, matching_rule, Rule, Summary, RULES};
pub use export::{write_dxf, write_xlsx};
pub use model::{Bounds, Dataset, Feature, Geometry, Vertex};
pub use parse::{load_job_features, parse_job_features};