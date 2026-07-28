use std::collections::{HashMap, HashSet, VecDeque};

use crate::actions::maven::coordinate::{Coordinate, Constraint, ResolvedCoordinate, compare_versions, unwrap_version_range};
use crate::actions::maven::error::Error;
use crate::actions::maven::manifest::Manifest;
use crate::actions::maven::pom::{self, ResolvedPom};
use crate::actions::maven::{cache, hash, repo, trust};
use crate::progress::Logger;

pub struct ResolvedLibrary {
	// not read by the current caller (build.rs only takes jar_path), kept as public API context
	#[allow(dead_code)]
	pub coordinate: ResolvedCoordinate,
	pub jar_path: Option<String>,
	// true when this came from stubs-libs (or transitively from one): classpath resolution only,
	// never a candidate for dexing into the output
	pub is_stub: bool,
}

struct QueueItem {
	coordinate: Coordinate,
	constraint: Constraint,
	// None for top-level libraries.yml entries, Some(parent) for transitive dependencies
	required_by: Option<ResolvedCoordinate>,
	// set when a pom named a dependency without any version haru could pin down: such a request
	// resolves to whatever is newest, so it may seed a coordinate nobody else asked for but must
	// never outrank a version that was actually named somewhere
	version_guessed: bool,
	// true when this item traces back to stubs-libs rather than libraries
	is_stub: bool,
}

struct PomCache {
	disk: cache::PomIndex,
	memory: HashMap<ResolvedCoordinate, (String, String)>,
}

impl PomCache {
	fn load() -> Result<Self, Error> {
		Ok(Self { disk: cache::PomIndex::load()?, memory: HashMap::new() })
	}

	fn get(&mut self, resolved: &ResolvedCoordinate) -> Option<(String, String)> {
		if let Some(cached) = self.memory.get(resolved) {
			return Some(cached.clone());
		}
		let (xml, source) = self.disk.get_xml(resolved)?;
		self.memory.insert(resolved.clone(), (xml.clone(), source.clone()));
		Some((xml, source))
	}

	fn insert(&mut self, resolved: &ResolvedCoordinate, xml: &str, source: &str) -> Result<(), Error> {
		self.memory.insert(resolved.clone(), (xml.to_string(), source.to_string()));

		let cache_path = cache::pom_cache_path(resolved);
		if let Some(parent) = cache_path.parent() {
			std::fs::create_dir_all(parent)?;
		}
		std::fs::write(&cache_path, xml)?;
		self.disk.insert(resolved, cache::PomIndexEntry { source: source.to_string(), file_path: cache_path.to_string_lossy().into_owned() });
		Ok(())
	}

	fn save(&self) -> Result<(), Error> {
		self.disk.save()
	}
}

pub fn resolve(manifest: &Manifest, logger: &mut Logger) -> Result<Vec<ResolvedLibrary>, Error> {
	let mut index = cache::Index::load()?;
	let mut pom_cache = PomCache::load()?;

	let result = resolve_queue(manifest, &mut index, &mut pom_cache, logger);

	let save_result = index.save().and_then(|()| pom_cache.save());

	let resolved_libraries = result?;
	save_result?;
	Ok(resolved_libraries)
}

struct Selection {
	resolved: ResolvedCoordinate,
	effective_pom: ResolvedPom,
	source: String,
	required_by: Option<ResolvedCoordinate>,
	is_stub: bool,
}

fn resolve_queue(manifest: &Manifest, index: &mut cache::Index, pom_cache: &mut PomCache, logger: &mut Logger) -> Result<Vec<ResolvedLibrary>, Error> {
	let discovered_total = discover_total(manifest, index, pom_cache, logger)?;
	logger.set_maven_total(discovered_total);

	let selections = select_versions(manifest, index, pom_cache, logger)?;
	let trusted: HashSet<ResolvedCoordinate> = manifest.trusted.iter().cloned().collect();

	let mut resolved_libraries: Vec<ResolvedLibrary> = Vec::new();
	for selection in selections {
		let Selection { resolved, effective_pom, source, required_by, is_stub } = selection;

		if let Some(dependency) = &required_by {
			if !trusted.contains(&resolved) {
				if !manifest.trust_system {
					logger.log(&format!("Installing transitive dependency {resolved}"));
				} else if !trust::ask_and_remember(logger, dependency, &resolved) {
					return Err(Error::TrustDenied { dependency: dependency.to_string(), needs: resolved.to_string() });
				}
			}
		}

		if effective_pom.packaging == "pom" {
			// a pom-only artifact (bom / parent aggregator) contributes no jar/aar to link against
			continue;
		}

		let jar_path = fetch_artifact(&resolved, &source, &effective_pom, index, logger)?;
		logger.maven_installed_step();
		resolved_libraries.push(ResolvedLibrary { coordinate: resolved, jar_path, is_stub });
	}

	logger.clear_maven_total();
	Ok(resolved_libraries)
}

// A coordinate can be asked for at several versions at once — once by maven.yml and again by
// whatever pulls it in transitively. Gradle settles that by letting the highest version win, and
// the stubs are the sources of a gradle build, so they only type-check against the same choice:
// com.google.guava:guava wants checker-qual 3.12.0 while maven.yml pins 2.5.2, and the telegram
// sources use a class that only exists in 3.x.
// Selection therefore runs to a fixpoint before anything is downloaded: a higher version replaces
// the one already picked and re-expands its dependencies, which terminates because a coordinate is
// only ever re-visited when its version strictly increases.
fn select_versions(manifest: &Manifest, index: &cache::Index, pom_cache: &mut PomCache, logger: &mut Logger) -> Result<Vec<Selection>, Error> {
	let mut selected: HashMap<Coordinate, Selection> = HashMap::new();
	// first-seen order, so the classpath stays stable across runs and keeps direct entries in front
	let mut order: Vec<Coordinate> = Vec::new();

	let mut queue: VecDeque<QueueItem> = VecDeque::new();
	for (coordinate, constraint) in &manifest.libraries {
		queue.push_back(QueueItem { coordinate: coordinate.clone(), constraint: constraint.clone(), required_by: None, version_guessed: false, is_stub: false });
	}
	for (coordinate, constraint) in &manifest.stub_libraries {
		queue.push_back(QueueItem { coordinate: coordinate.clone(), constraint: constraint.clone(), required_by: None, version_guessed: false, is_stub: true });
	}

	while let Some(item) = queue.pop_front() {
		let already_selected = selected.contains_key(&item.coordinate);
		if item.version_guessed && already_selected {
			continue;
		}

		let version = find_version(manifest, &item.coordinate, &item.constraint, index, logger)?;

		let mut required_by = item.required_by;
		// a coordinate reached by any non-stub path is never left as a stub: stub status only
		// applies when every path requesting it is a stub, so a real dependency always wins
		let mut is_stub = item.is_stub;
		let mut superseded = false;
		match selected.get(&item.coordinate) {
			None => order.push(item.coordinate.clone()),
			Some(current) => {
				is_stub = current.is_stub && item.is_stub;
				if compare_versions(&current.resolved.version, &version) != std::cmp::Ordering::Less {
					superseded = true;
				} else {
					logger.log(&format!("{} {} superseded by {version}", item.coordinate, current.resolved.version));
					// a library named in maven.yml stays a direct one even when a transitive bumps it
					if current.required_by.is_none() {
						required_by = None;
					}
				}
			}
		}
		if superseded {
			if let Some(current) = selected.get_mut(&item.coordinate) {
				current.is_stub = is_stub;
			}
			continue;
		}

		let resolved = ResolvedCoordinate {
			group_id: item.coordinate.group_id.clone(),
			artifact_id: item.coordinate.artifact_id.clone(),
			version,
		};

		logger.log(&format!("Resolved {resolved}"));

		let (effective_pom, source) = fetch_pom_chain(manifest, &resolved, pom_cache, logger)?;

		if manifest.transit {
			for dependency in &effective_pom.dependencies {
				if !dependency.needed_at_runtime() {
					continue;
				}
				let (constraint, version_guessed) = match &dependency.version {
					Some(version) => (Constraint::Eq(unwrap_version_range(version)), false),
					None => {
						logger.log(&format!("No version resolved for {}:{} in {resolved} (missing dependencyManagement entry), falling back to latest", dependency.group_id, dependency.artifact_id));
						(Constraint::Latest, true)
					}
				};
				let child_coordinate = Coordinate { group_id: dependency.group_id.clone(), artifact_id: dependency.artifact_id.clone() };
				queue.push_back(QueueItem { coordinate: child_coordinate, constraint, required_by: Some(resolved.clone()), version_guessed, is_stub });
			}
		}

		selected.insert(item.coordinate, Selection { resolved, effective_pom, source, required_by, is_stub });
	}

	Ok(order.into_iter().filter_map(|coordinate| selected.remove(&coordinate)).collect())
}

fn discover_total(manifest: &Manifest, index: &cache::Index, pom_cache: &mut PomCache, logger: &mut Logger) -> Result<u32, Error> {
	let mut visited: HashSet<Coordinate> = HashSet::new();

	for (coordinate, constraint) in manifest.libraries.iter().chain(&manifest.stub_libraries) {
		logger.log(&format!("Finding all sub-dependencies for {coordinate}"));

		let mut queue: VecDeque<QueueItem> = VecDeque::new();
		queue.push_back(QueueItem { coordinate: coordinate.clone(), constraint: constraint.clone(), required_by: None, version_guessed: false, is_stub: false });

		while let Some(item) = queue.pop_front() {
			if visited.contains(&item.coordinate) {
				continue;
			}
			visited.insert(item.coordinate.clone());

			let resolved_version = find_version(manifest, &item.coordinate, &item.constraint, index, logger)?;
			let resolved = ResolvedCoordinate {
				group_id: item.coordinate.group_id.clone(),
				artifact_id: item.coordinate.artifact_id.clone(),
				version: resolved_version,
			};

			let (effective_pom, _) = fetch_pom_chain(manifest, &resolved, pom_cache, logger)?;

			if !manifest.transit {
				continue;
			}
			for dependency in &effective_pom.dependencies {
				if !dependency.needed_at_runtime() {
					continue;
				}
				let constraint = match &dependency.version {
					Some(version) => Constraint::Eq(unwrap_version_range(version)),
					None => Constraint::Latest,
				};
				let child_coordinate = Coordinate { group_id: dependency.group_id.clone(), artifact_id: dependency.artifact_id.clone() };
				queue.push_back(QueueItem { coordinate: child_coordinate, constraint, required_by: Some(resolved.clone()), version_guessed: false, is_stub: false });
			}
		}
	}

	Ok(visited.len() as u32)
}

fn find_version(manifest: &Manifest, coordinate: &Coordinate, constraint: &Constraint, index: &cache::Index, logger: &mut Logger) -> Result<String, Error> {
	if let Constraint::Eq(version) = constraint {
		return Ok(version.clone());
	}

	if let Some(cached_version) = cached_version_satisfying(coordinate, constraint, index) {
		logger.debug(&format!("{coordinate}: using cached version {cached_version}, satisfies constraint"));
		return Ok(cached_version);
	}

	let mut best: Option<(String, usize)> = None;
	for (source_index, source) in manifest.sources.iter().enumerate() {
		let metadata_url = repo::metadata_url(source, &coordinate.group_id, &coordinate.artifact_id);
		let Some(xml) = repo::fetch_text(&metadata_url)? else {
			continue;
		};
		let versions = crate::actions::maven::metadata::parse_all_versions(&xml, &coordinate.key())?;
		let matching = versions.into_iter().filter(|v| satisfies(v, constraint)).max_by(|a, b| compare_versions(a, b));

		let Some(candidate) = matching else {
			continue;
		};

		let better_than_current = match &best {
			None => true,
			Some((current_version, current_source_index)) => {
				*current_source_index > source_index || compare_versions(&candidate, current_version) == std::cmp::Ordering::Greater
			}
		};
		if better_than_current {
			best = Some((candidate, source_index));
		}

		if !manifest.check_across_all_repos {
			break;
		}
	}

	best.map(|(version, _)| version).ok_or_else(|| Error::NotFound { coordinate: coordinate.to_string() })
}

fn satisfies(version: &str, constraint: &Constraint) -> bool {
	match constraint {
		Constraint::Eq(required) => compare_versions(version, required) == std::cmp::Ordering::Equal,
		Constraint::Ge(required) => compare_versions(version, required) != std::cmp::Ordering::Less,
		Constraint::Le(required) => compare_versions(version, required) != std::cmp::Ordering::Greater,
		Constraint::Latest => true,
	}
}

fn cached_version_satisfying(coordinate: &Coordinate, constraint: &Constraint, index: &cache::Index) -> Option<String> {
	let mut candidates: Vec<String> = index
		.versions_for(coordinate)
		.into_iter()
		.filter(|version| satisfies(version, constraint))
		.filter(|version| {
			let resolved = ResolvedCoordinate { group_id: coordinate.group_id.clone(), artifact_id: coordinate.artifact_id.clone(), version: version.clone() };
			index.get(&resolved).is_some_and(cache::is_entry_valid)
		})
		.collect();

	candidates.sort_by(|a, b| compare_versions(a, b));
	candidates.pop()
}

fn fetch_pom_chain(manifest: &Manifest, resolved: &ResolvedCoordinate, pom_cache: &mut PomCache, logger: &mut Logger) -> Result<(ResolvedPom, String), Error> {
	let (raw, xml, source) = fetch_pom_raw(manifest, resolved, pom_cache, logger)?;

	let parent_resolved = match &raw.parent {
		Some(parent) => {
			let parent_coordinate =
				ResolvedCoordinate { group_id: parent.group_id.clone(), artifact_id: parent.artifact_id.clone(), version: parent.version.clone() };
			let (parent_pom, _) = fetch_pom_chain(manifest, &parent_coordinate, pom_cache, logger)?;
			Some(parent_pom)
		}
		None => None,
	};

	let label = resolved.to_string();
	let resolved_pom = pom::resolve(raw, &xml, parent_resolved.as_ref(), &label)?;
	Ok((resolved_pom, source))
}

fn fetch_pom_raw(manifest: &Manifest, resolved: &ResolvedCoordinate, pom_cache: &mut PomCache, logger: &mut Logger) -> Result<(pom::RawPom, String, String), Error> {
	if let Some((xml, source)) = pom_cache.get(resolved) {
		logger.debug(&format!("{resolved}: using cached pom from {source}"));
		let raw = pom::parse(&xml, &resolved.to_string())?;
		return Ok((raw, xml, source));
	}

	for source in &manifest.sources {
		let url = repo::artifact_file_url(source, resolved, None, "pom");
		let Some(xml) = repo::fetch_text(&url)? else {
			continue;
		};
		logger.debug(&format!("fetched pom for {resolved} from {source}"));
		let raw = pom::parse(&xml, &resolved.to_string())?;
		pom_cache.insert(resolved, &xml, source)?;
		return Ok((raw, xml, source.clone()));
	}
	Err(Error::NotFound { coordinate: format!("{resolved} (pom)") })
}

fn fetch_artifact(resolved: &ResolvedCoordinate, source: &str, effective_pom: &ResolvedPom, index: &mut cache::Index, logger: &mut Logger) -> Result<Option<String>, Error> {
	let extension = if effective_pom.packaging == "aar" { "aar" } else { "jar" };

	if let Some(entry) = index.get(resolved) {
		if entry.checksum_algorithm != "none" && cache::is_entry_valid(entry) {
			logger.log(&format!("Using cached {resolved}"));
			return prepare_for_linking(resolved, &entry.file_path.clone(), extension, logger);
		}
	}

	let artifact_url = repo::artifact_file_url(source, resolved, None, extension);
	let Some(bytes) = repo::fetch_bytes(&artifact_url)? else {
		return Err(Error::NotFound { coordinate: resolved.to_string() });
	};

	let checksum = repo::fetch_checksum(&artifact_url)?;
	let (algorithm_name, checksum_hex) = match checksum {
		Some((algorithm, expected_hex)) => {
			let actual_hex = hash::digest_hex(algorithm, &bytes);
			if actual_hex != expected_hex {
				return Err(Error::ChecksumMismatch { coordinate: resolved.to_string(), file: artifact_url });
			}
			(algorithm.extension().to_string(), actual_hex)
		}
		None => {
			logger.log(&format!("Warn: no checksum available for {resolved}, downloading unverified"));
			("none".to_string(), String::new())
		}
	};

	let cache_path = cache::artifact_cache_path(resolved, extension);
	if let Some(parent) = cache_path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(&cache_path, &bytes)?;

	let file_path = cache_path.to_string_lossy().into_owned();
	index.insert(resolved, cache::IndexEntry { source: source.to_string(), file_path: file_path.clone(), packaging: effective_pom.packaging.clone(), checksum_algorithm: algorithm_name, checksum_hex });

	logger.log(&format!("Downloaded {resolved}"));
	prepare_for_linking(resolved, &file_path, extension, logger)
}

fn prepare_for_linking(resolved: &ResolvedCoordinate, file_path: &str, extension: &str, logger: &mut Logger) -> Result<Option<String>, Error> {
	if extension != "aar" {
		return Ok(Some(file_path.to_string()));
	}

	let aar_path = std::path::Path::new(file_path);
	let out_dir = aar_path.with_extension("");
	let out_jar = out_dir.join("classes.jar");

	let file = std::fs::File::open(aar_path)?;
	let mut archive = zip::ZipArchive::new(file).map_err(|err| Error::Io(std::io::Error::other(err)))?;
	let Ok(mut entry) = archive.by_name("classes.jar") else {
		logger.log(&format!("Warn: {resolved} has no classes.jar inside its aar, nothing to add to the classpath"));
		return Ok(None);
	};

	std::fs::create_dir_all(&out_dir)?;
	let mut out_file = std::fs::File::create(&out_jar)?;
	std::io::copy(&mut entry, &mut out_file)?;

	Ok(Some(out_jar.to_string_lossy().into_owned()))
}
