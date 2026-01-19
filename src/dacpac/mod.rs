//! Dacpac generation

mod metadata_xml;
mod model_xml;
mod origin_xml;
mod packager;

pub use packager::create_dacpac;
