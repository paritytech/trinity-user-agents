/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::PresentationProfile;
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;

/// Validated host-facing description of a strict App manifest v2 executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppDescriptor {
    /// Semantic application version.
    pub app_version: Vec<u32>,
    /// PolkaVM entrypoint within the verified application archive.
    pub program_path: String,
    /// Selected presentation profile.
    pub presentation: PresentationProfile,
    /// Whether the application may submit audio.
    pub audio_enabled: bool,
    /// Required device-input features.
    pub input_features: Vec<String>,
    /// Required WebGPU limits, empty for other profiles.
    pub gpu_limits: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(rename = "$v")]
    version: u32,
    kind: String,
    #[serde(rename = "appVersion")]
    app_version: Vec<u32>,
    runtime: Runtime,
    capabilities: Capabilities,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Runtime {
    kind: String,
    #[serde(rename = "abiVersion")]
    abi_version: u32,
    entrypoint: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capabilities {
    graphics: Graphics,
    #[serde(rename = "deviceInput")]
    device_input: Option<DeviceInput>,
    audio: Option<Audio>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Graphics {
    #[serde(rename = "abiVersion")]
    abi_version: u32,
    profile: String,
    #[serde(rename = "requiredFeatures")]
    required_features: Vec<String>,
    #[serde(rename = "requiredLimits", default)]
    required_limits: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceInput {
    #[serde(rename = "abiVersion")]
    abi_version: u32,
    #[serde(rename = "requiredFeatures")]
    required_features: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Audio {
    #[serde(rename = "abiVersion")]
    abi_version: u32,
    #[serde(rename = "requiredFeatures")]
    required_features: Vec<String>,
}

const GPU_LIMITS: &[&str] = &[
    "maxTextureDimension2D",
    "maxBufferSize",
    "maxBindingsPerBindGroup",
    "maxBindGroups",
    "maxVertexBuffers",
    "maxVertexAttributes",
    "maxColorAttachments",
];

impl AppDescriptor {
    /// Parses and validates an embedded App v2 manifest after proving it is
    /// byte-identical to the executable record resolved from DotNS.
    pub fn parse_exact(embedded: &[u8], executable_record: &[u8]) -> Result<Self> {
        if embedded != executable_record {
            bail!("embedded App manifest differs from executable record");
        }
        let manifest: Manifest =
            serde_json::from_slice(embedded).context("parse strict App manifest v2")?;
        if manifest.version != 2 || manifest.kind != "app" {
            bail!("manifest must be $v 2 kind app");
        }
        if !(manifest.app_version.len() == 3 || manifest.app_version.len() == 4) {
            bail!("App version must contain three or four components");
        }
        if manifest.runtime.kind != "polkavm" || manifest.runtime.abi_version != 1 {
            bail!("App runtime must be PolkaVM ABI version 1");
        }
        validate_path(&manifest.runtime.entrypoint)?;
        if !manifest.runtime.entrypoint.ends_with(".polkavm") {
            bail!("PolkaVM entrypoint must end in .polkavm");
        }
        if manifest.capabilities.graphics.abi_version != 1 {
            bail!("graphics capability must use ABI version 1");
        }
        if !manifest.capabilities.graphics.required_features.is_empty() {
            bail!("graphics profile requests unsupported optional features");
        }
        let presentation = PresentationProfile::parse(&manifest.capabilities.graphics.profile)?;
        let gpu_limits = manifest.capabilities.graphics.required_limits;
        if presentation == PresentationProfile::WebGpuRaster {
            for (name, value) in &gpu_limits {
                if !GPU_LIMITS.contains(&name.as_str()) || *value == 0 {
                    return Err(anyhow!("unsupported WebGPU required limit {name}"));
                }
            }
        } else if !gpu_limits.is_empty() {
            bail!("non-WebGPU graphics profile declares required limits");
        }
        let input_features = if let Some(input) = manifest.capabilities.device_input {
            if input.abi_version != 1 {
                bail!("device input capability must use ABI version 1");
            }
            for feature in &input.required_features {
                if feature != "pointer" && feature != "keyboard" {
                    bail!("unsupported device input feature {feature}");
                }
            }
            input.required_features
        } else {
            Vec::new()
        };
        let audio_enabled = if let Some(audio) = manifest.capabilities.audio {
            if audio.abi_version != 1 || !audio.required_features.is_empty() {
                bail!("audio capability requires unsupported features or ABI");
            }
            true
        } else {
            false
        };
        Ok(Self {
            app_version: manifest.app_version,
            program_path: manifest.runtime.entrypoint,
            presentation,
            audio_enabled,
            input_features,
            gpu_limits,
        })
    }
}

fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        bail!("invalid application asset path {path}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::AppDescriptor;
    use crate::PresentationProfile;

    const FRAMEBUFFER: &[u8] = br#"{"$v":2,"kind":"app","appVersion":[1,2,3],"runtime":{"kind":"polkavm","abiVersion":1,"entrypoint":"app.polkavm"},"capabilities":{"graphics":{"abiVersion":1,"profile":"framebuffer","requiredFeatures":[]},"deviceInput":{"abiVersion":1,"requiredFeatures":["pointer","keyboard"]},"audio":{"abiVersion":1,"requiredFeatures":[]}}}"#;

    #[test]
    fn parses_exact_strict_manifest() {
        let descriptor = AppDescriptor::parse_exact(FRAMEBUFFER, FRAMEBUFFER).unwrap();
        assert_eq!(descriptor.presentation, PresentationProfile::Framebuffer);
        assert_eq!(descriptor.program_path, "app.polkavm");
        assert!(descriptor.audio_enabled);
    }

    #[test]
    fn rejects_external_byte_mismatch() {
        let mut changed = FRAMEBUFFER.to_vec();
        changed.push(b'\n');
        assert!(AppDescriptor::parse_exact(FRAMEBUFFER, &changed).is_err());
    }

    #[test]
    fn rejects_legacy_and_unknown_fields() {
        let legacy = FRAMEBUFFER
            .windows(6)
            .position(|window| window == b"\"$v\":2")
            .map(|offset| {
                let mut bytes = FRAMEBUFFER.to_vec();
                bytes[offset + 5] = b'1';
                bytes
            })
            .unwrap();
        assert!(AppDescriptor::parse_exact(&legacy, &legacy).is_err());
        let unknown = String::from_utf8(FRAMEBUFFER.to_vec())
            .unwrap()
            .replace("\"kind\":\"app\"", "\"kind\":\"app\",\"modalities\":{}");
        assert!(AppDescriptor::parse_exact(unknown.as_bytes(), unknown.as_bytes()).is_err());
    }
}
