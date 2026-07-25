#[derive(Debug)]
pub enum Error {
	MavenYmlNotFound,
	MavenYmlInvalid(String),
	InvalidCoordinate(String),
	NotFound { coordinate: String },
	PomInvalid { coordinate: String, reason: String },
	MetadataInvalid { coordinate: String, reason: String },
	ChecksumMismatch { coordinate: String, file: String },
	NoChecksumAvailable { coordinate: String, file: String },
	TrustDenied { dependency: String, needs: String },
	VersionConflict { coordinate: String, wanted: String, resolved: String },
	Network { url: String, reason: String },
	Io(std::io::Error),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::MavenYmlNotFound => write!(f, "maven.yml not found"),
			Self::MavenYmlInvalid(reason) => write!(f, "maven.yml is invalid: {reason}"),
			Self::InvalidCoordinate(raw) => write!(f, "invalid maven coordinate: \"{raw}\""),
			Self::NotFound { coordinate } => write!(f, "library not found in any configured source: {coordinate}"),
			Self::PomInvalid { coordinate, reason } => write!(f, "pom for {coordinate} is invalid: {reason}"),
			Self::MetadataInvalid { coordinate, reason } => write!(f, "maven-metadata.xml for {coordinate} is invalid: {reason}"),
			Self::ChecksumMismatch { coordinate, file } => write!(f, "checksum mismatch for {coordinate} ({file})"),
			Self::NoChecksumAvailable { coordinate, file } => write!(f, "no checksum available for {coordinate} ({file})"),
			Self::TrustDenied { dependency, needs } => write!(f, "{dependency} needs {needs}, but it was not trusted"),
			Self::VersionConflict { coordinate, wanted, resolved } => {
				write!(f, "{coordinate}: wanted {wanted}, but {resolved} was already resolved from a higher priority source")
			}
			Self::Network { url, reason } => write!(f, "network error fetching {url}: {reason}"),
			Self::Io(err) => write!(f, "{err}"),
		}
	}
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Self {
		Self::Io(err)
	}
}
