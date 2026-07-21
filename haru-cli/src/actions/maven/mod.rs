mod cache;
mod coordinate;
mod error;
mod hash;
mod manifest;
mod metadata;
mod pom;
mod repo;
mod resolve;
mod trust;

pub use error::Error;
pub use resolve::ResolvedLibrary;

use crate::progress::Logger;

pub fn resolve(logger: &mut Logger) -> Result<Vec<ResolvedLibrary>, Error> {
	let Some(loaded) = manifest::load()? else {
		return Ok(Vec::new());
	};
	if loaded.libraries.is_empty() {
		return Ok(Vec::new());
	}
	resolve::resolve(&loaded, logger)
}
