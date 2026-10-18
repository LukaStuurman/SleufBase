//! SleufBase Rust port — local KickTheMap data model and exports.
//! The historical Python application remains unchanged until parity is reached.

pub mod disciplines;
pub mod geo;
pub mod geotiff;
pub mod loaded_jobs;
pub mod raster;
pub mod raster_loader;
pub mod file_loader;
mod input;
mod raster_color;
mod tiff_view;
pub mod dxf_read;
pub mod cad;
mod cad_layers;
pub mod cad_geometry;
pub mod cad_mesh;
pub mod cad_scene;
pub mod spline;
mod dxf_tags;
mod dxf_entities;
mod dxf_mesh;
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
pub mod leader_index;
pub mod marxact_alignment;
pub mod cadastral;
pub mod settings_compat;
pub mod export;
pub mod object_dxf;
mod object_layout;
pub mod model;
pub mod parse;

pub use disciplines::{summarize, matching_rule, Rule, Summary, RULES};
pub use export::{write_dxf, write_dxf_with_trenches, write_dxf_with_layers, write_xlsx};
pub use model::{Bounds, Dataset, Feature, Geometry, Vertex};
pub use parse::{load_job_features, parse_job_features};
