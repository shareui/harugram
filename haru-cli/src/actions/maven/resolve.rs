use std::collections::{HashMap, HashSet, VecDeque};

use crate::actions::maven::coordinate::{Coordinate, Constraint, ResolvedCoordinate, compare_versions, unwrap_version_range};
use crate::actions::maven::error::Error;
use crate::actions::maven::manifest::Manifest;
use crate::actions::maven::pom::{self, ResolvedPom};
use crate::actions::maven::{cache, hash, repo, trust};
use crate::progress::Logger;

pub struct ResolvedLibrary {
	pub coordinate: ResolvedCoordinate,
	pub jar_path: Option<String>,
}

struct QueueItem {
	coordinate: Coordinate,
	constraint: Constraint,
	// None for top-level libraries.yml entries, Some(parent) for transitive dependencies
	required_by: Option<ResolvedCoordinate>,
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

fn resolve_queue(manifest: &Manifest, index: &mut cache::Index, pom_cache: &mut PomCache, logger: &mut Logger) -> Result<Vec<ResolvedLibrary>, Error> {
	let discovered_total = discover_total(manifest, index, pom_cache, logger)?;
	logger.set_maven_total(discovered_total);

	let mut resolved_libraries: Vec<ResolvedLibrary> = Vec::new();
	let mut visited: HashSet<Coordinate> = HashSet::new();
	let trusted: HashSet<ResolvedCoordinate> = manifest.trusted.iter().cloned().collect();

	let mut queue: VecDeque<QueueItem> = VecDeque::new();
	for (coordinate, constraint) in &manifest.libraries {
		queue.push_back(QueueItem { coordinate: coordinate.clone(), constraint: constraint.clone(), required_by: None });
	}

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

		if let Some(dependency) = &item.required_by {
			if !trusted.contains(&resolved) {
				if !manifest.trust_system {
					logger.log(&format!("Installing transitive dependency {resolved}"));
				} else if !trust::ask_and_remember(logger, dependency, &resolved) {
					return Err(Error::TrustDenied { dependency: dependency.to_string(), needs: resolved.to_string() });
				}
			}
		}

		logger.log(&format!("Resolved {resolved}"));

		let (effective_pom, source) = fetch_pom_chain(manifest, &resolved, pom_cache, logger)?;

		if manifest.transit {
			for dependency in &effective_pom.dependencies {
				if !dependency.needed_at_runtime() {
					continue;
				}
				let constraint = match &dependency.version {
					Some(version) => Constraint::Eq(unwrap_version_range(version)),
					None => {
						logger.log(&format!("No version resolved for {}:{} in {resolved} (missing dependencyManagement entry), falling back to latest", dependency.group_id, dependency.artifact_id));
						Constraint::Latest
					}
				};
				let child_coordinate = Coordinate { group_id: dependency.group_id.clone(), artifact_id: dependency.artifact_id.clone() };
				queue.push_back(QueueItem { coordinate: child_coordinate, constraint, required_by: Some(resolved.clone()) });
			}
		}

		if effective_pom.packaging == "pom" {
			// a pom-only artifact (bom / parent aggregator) contributes no jar/aar to link against
			continue;
		}

		let jar_path = fetch_artifact(&resolved, &source, &effective_pom, index, logger)?;
		logger.maven_installed_step();
		resolved_libraries.push(ResolvedLibrary { coordinate: resolved, jar_path });
	}

	logger.clear_maven_total();
	Ok(resolved_libraries)
}

fn discover_total(manifest: &Manifest, index: &cache::Index, pom_cache: &mut PomCache, logger: &mut Logger) -> Result<u32, Error> {
	let mut visited: HashSet<Coordinate> = HashSet::new();

	for (coordinate, constraint) in &manifest.libraries {
		logger.log(&format!("Finding all sub-dependencies for {coordinate}"));

		let mut queue: VecDeque<QueueItem> = VecDeque::new();
		queue.push_back(QueueItem { coordinate: coordinate.clone(), constraint: constraint.clone(), required_by: None });

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
				queue.push_back(QueueItem { coordinate: child_coordinate, constraint, required_by: Some(resolved.clone()) });
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
