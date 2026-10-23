//! Which extrinsic format and extension pipeline a build uses.

use subxt::{Metadata, ext::frame_decode::extrinsics::ExtrinsicTypeInfo};

use super::{error::ExtrinsicBuildError, well_known::VERIFY_MULTI_SIGNATURE};

/// The extrinsic format and, for V5, the extension pipeline it uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtrinsicVersion {
	/// A V4 signed extrinsic. It always uses extension pipeline 0.
	V4,
	/// A V5 general extrinsic using the given extension pipeline.
	V5 {
		/// The pipeline's number in metadata.
		extension_version: u8,
	},
}

/// How a caller pins the version instead of taking the host's default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionChoice {
	/// Exactly this version.
	Exact(ExtrinsicVersion),
	/// The newest V5 pipeline that declares the named extension.
	NewestDeclaring(&'static str),
}

impl ExtrinsicVersion {
	/// The newest V5 pipeline that declares `extension`, resolved against
	/// metadata when the extrinsic is built.
	pub fn newest_declaring(extension: &'static str) -> VersionChoice {
		VersionChoice::NewestDeclaring(extension)
	}

	/// The extension pipeline this version encodes with.
	pub fn extension_version(self) -> u8 {
		match self {
			Self::V4 => 0,
			Self::V5 { extension_version } => extension_version,
		}
	}
}

/// The version a build uses: the caller's pin if there is one, else the
/// host's default for the chain.
///
/// The host default follows the native hosts: a chain the host maps to a
/// pipeline is V5 with that pipeline, unless the build is signed and the
/// pipeline has no `VerifyMultiSignature`. Every other chain is V4.
pub fn choose_version(
	choice: Option<VersionChoice>,
	host_extension_version: Option<u8>,
	signed: bool,
	metadata: &Metadata,
) -> Result<ExtrinsicVersion, ExtrinsicBuildError> {
	match choice {
		Some(VersionChoice::Exact(version)) => Ok(version),
		Some(VersionChoice::NewestDeclaring(extension)) => newest_declaring(extension, metadata),
		None => host_default(host_extension_version, signed, metadata),
	}
}

fn host_default(
	host_extension_version: Option<u8>,
	signed: bool,
	metadata: &Metadata,
) -> Result<ExtrinsicVersion, ExtrinsicBuildError> {
	let Some(extension_version) = host_extension_version else {
		return Ok(ExtrinsicVersion::V4);
	};
	if signed && !declares(metadata, extension_version, VERIFY_MULTI_SIGNATURE)? {
		return Ok(ExtrinsicVersion::V4);
	}
	Ok(ExtrinsicVersion::V5 { extension_version })
}

fn newest_declaring(
	extension: &str,
	metadata: &Metadata,
) -> Result<ExtrinsicVersion, ExtrinsicBuildError> {
	let mut declaring = Vec::new();
	for extension_version in declared_pipelines(metadata)? {
		if declares(metadata, extension_version, extension)? {
			declaring.push(extension_version);
		}
	}
	let newest =
		declaring
			.into_iter()
			.max()
			.ok_or_else(|| ExtrinsicBuildError::NoPipelineDeclares {
				extension: extension.to_string(),
			})?;
	Ok(ExtrinsicVersion::V5 { extension_version: newest })
}

/// The extension pipelines the runtime declares.
pub fn declared_pipelines(metadata: &Metadata) -> Result<Vec<u8>, ExtrinsicBuildError> {
	metadata
		.extrinsic_extension_version_info()
		.map(Iterator::collect)
		.map_err(|error| ExtrinsicBuildError::Metadata(format!("extension pipelines: {error}")))
}

/// Whether pipeline `extension_version` lists `extension`.
pub fn declares(
	metadata: &Metadata,
	extension_version: u8,
	extension: &str,
) -> Result<bool, ExtrinsicBuildError> {
	Ok(pipeline_names(metadata, extension_version)?
		.iter()
		.any(|name| name == extension))
}

/// The names of pipeline `extension_version`'s extensions, in order.
pub fn pipeline_names(
	metadata: &Metadata,
	extension_version: u8,
) -> Result<Vec<String>, ExtrinsicBuildError> {
	let pipeline = metadata.extrinsic_extension_info(Some(extension_version)).map_err(|error| {
		ExtrinsicBuildError::UnknownPipeline { extension_version, reason: error.to_string() }
	})?;
	Ok(pipeline
		.extension_ids
		.into_iter()
		.map(|declared| declared.name.into_owned())
		.collect())
}
