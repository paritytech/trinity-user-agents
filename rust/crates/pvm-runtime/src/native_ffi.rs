/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    ApplicationRuntime, AudioChunk, Frame, GpuBatch, InputEvent, InputEventType,
    PresentationProfile, Tri2dFrame,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum NativePvmPresentationProfile {
    Framebuffer,
    Tri2d,
    WebGpuRaster,
}

impl From<NativePvmPresentationProfile> for PresentationProfile {
    fn from(value: NativePvmPresentationProfile) -> Self {
        match value {
            NativePvmPresentationProfile::Framebuffer => Self::Framebuffer,
            NativePvmPresentationProfile::Tri2d => Self::Tri2d,
            NativePvmPresentationProfile::WebGpuRaster => Self::WebGpuRaster,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum NativePvmInputEventType {
    KeyDown,
    KeyUp,
    ButtonDown,
    ButtonUp,
    PointerMove,
    PointerDelta,
    SurfaceMetrics,
}

impl From<NativePvmInputEventType> for InputEventType {
    fn from(value: NativePvmInputEventType) -> Self {
        match value {
            NativePvmInputEventType::KeyDown => Self::KeyDown,
            NativePvmInputEventType::KeyUp => Self::KeyUp,
            NativePvmInputEventType::ButtonDown => Self::ButtonDown,
            NativePvmInputEventType::ButtonUp => Self::ButtonUp,
            NativePvmInputEventType::PointerMove => Self::PointerMove,
            NativePvmInputEventType::PointerDelta => Self::PointerDelta,
            NativePvmInputEventType::SurfaceMetrics => Self::SurfaceMetrics,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NativePvmAsset {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NativePvmFrame {
    pub width: u32,
    pub height: u32,
    pub argb: Vec<u8>,
}

impl From<Frame> for NativePvmFrame {
    fn from(frame: Frame) -> Self {
        Self {
            width: frame.width,
            height: frame.height,
            argb: frame.argb,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NativePvmTri2dFrame {
    pub width: u32,
    pub height: u32,
    pub draw_count: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub bytes: Vec<u8>,
}

impl From<Tri2dFrame> for NativePvmTri2dFrame {
    fn from(frame: Tri2dFrame) -> Self {
        Self {
            width: frame.width,
            height: frame.height,
            draw_count: frame.draw_count,
            vertex_count: frame.vertex_count,
            index_count: frame.index_count,
            bytes: frame.bytes,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NativePvmAudioChunk {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u32,
}

impl From<AudioChunk> for NativePvmAudioChunk {
    fn from(chunk: AudioChunk) -> Self {
        Self {
            samples: chunk.samples,
            sample_rate: chunk.sample_rate,
            channels: chunk.channels,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct NativePvmGpuBatch {
    pub bytes: Vec<u8>,
}

impl From<GpuBatch> for NativePvmGpuBatch {
    fn from(batch: GpuBatch) -> Self {
        Self { bytes: batch.bytes }
    }
}

#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
pub enum NativePvmError {
    #[error("{detail}")]
    Runtime { detail: String },
    #[error("asset path appears more than once: {path}")]
    DuplicateAsset { path: String },
    #[error("PVM runtime mutex was poisoned")]
    RuntimePoisoned,
}

impl NativePvmError {
    fn runtime(error: impl std::fmt::Display) -> Self {
        Self::Runtime {
            detail: error.to_string(),
        }
    }
}

#[derive(uniffi::Object)]
pub struct NativePvmRuntime {
    runtime: Mutex<ApplicationRuntime>,
}

impl NativePvmRuntime {
    fn lock(&self) -> Result<MutexGuard<'_, ApplicationRuntime>, NativePvmError> {
        self.runtime
            .lock()
            .map_err(|_| NativePvmError::RuntimePoisoned)
    }
}

#[uniffi::export]
impl NativePvmRuntime {
    #[uniffi::constructor]
    pub fn new(
        program: Vec<u8>,
        assets: Vec<NativePvmAsset>,
        presentation: NativePvmPresentationProfile,
        audio_enabled: bool,
        max_gas_per_update: u64,
    ) -> Result<Arc<Self>, NativePvmError> {
        crate::validate_program_configuration(program.len(), max_gas_per_update)
            .map_err(NativePvmError::runtime)?;
        crate::validate_asset_count(assets.len()).map_err(NativePvmError::runtime)?;
        let mut asset_map = HashMap::with_capacity(assets.len());
        for asset in assets {
            let path = asset.path;
            if asset_map.insert(path.clone(), asset.bytes).is_some() {
                return Err(NativePvmError::DuplicateAsset { path });
            }
        }
        let runtime = ApplicationRuntime::new(
            &program,
            asset_map,
            presentation.into(),
            audio_enabled,
            max_gas_per_update,
        )
        .map_err(NativePvmError::runtime)?;
        Ok(Arc::new(Self {
            runtime: Mutex::new(runtime),
        }))
    }

    pub fn init(&self) -> Result<(), NativePvmError> {
        self.lock()?.init().map_err(NativePvmError::runtime)
    }

    pub fn update(&self) -> Result<(), NativePvmError> {
        self.lock()?.update().map_err(NativePvmError::runtime)
    }

    pub fn backend(&self) -> Result<String, NativePvmError> {
        Ok(format!("{:?}", self.lock()?.backend()).to_ascii_lowercase())
    }

    pub fn last_gas_used(&self) -> Result<u64, NativePvmError> {
        Ok(self.lock()?.last_gas_used())
    }

    pub fn send_input(
        &self,
        event_type: NativePvmInputEventType,
        code: u8,
        x: u16,
        y: u16,
    ) -> Result<(), NativePvmError> {
        self.lock()?.send_input(InputEvent {
            event_type: event_type.into(),
            code,
            x,
            y,
        });
        Ok(())
    }

    pub fn gpu_ready(&self) -> Result<bool, NativePvmError> {
        Ok(self.lock()?.gpu_ready())
    }

    pub fn set_gpu_capabilities(&self, bytes: Vec<u8>) -> Result<(), NativePvmError> {
        self.lock()?
            .set_gpu_capabilities(bytes)
            .map_err(NativePvmError::runtime)
    }

    pub fn send_gpu_event(&self, bytes: Vec<u8>) -> Result<(), NativePvmError> {
        self.lock()?
            .send_gpu_event(bytes)
            .map_err(NativePvmError::runtime)
    }

    pub fn take_frame(&self) -> Result<Option<NativePvmFrame>, NativePvmError> {
        Ok(self.lock()?.take_frame().map(Into::into))
    }

    pub fn take_tri2d(&self) -> Result<Option<NativePvmTri2dFrame>, NativePvmError> {
        Ok(self.lock()?.take_tri2d().map(Into::into))
    }

    pub fn take_audio(&self) -> Result<Option<NativePvmAudioChunk>, NativePvmError> {
        Ok(self.lock()?.take_audio().map(Into::into))
    }

    pub fn take_gpu_batch(&self) -> Result<Option<NativePvmGpuBatch>, NativePvmError> {
        Ok(self.lock()?.take_gpu_batch().map(Into::into))
    }

    pub fn take_log(&self) -> Result<Option<String>, NativePvmError> {
        Ok(self.lock()?.take_log())
    }

    pub fn is_exited(&self) -> Result<bool, NativePvmError> {
        Ok(self.lock()?.is_exited())
    }

    pub fn take_save(&self) -> Result<Option<Vec<u8>>, NativePvmError> {
        Ok(self.lock()?.take_save())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_assets_are_rejected_before_program_parsing() {
        let result = NativePvmRuntime::new(
            Vec::new(),
            vec![
                NativePvmAsset {
                    path: "data.bin".into(),
                    bytes: vec![1],
                },
                NativePvmAsset {
                    path: "data.bin".into(),
                    bytes: vec![2],
                },
            ],
            NativePvmPresentationProfile::Framebuffer,
            false,
            1,
        );
        assert!(matches!(result, Err(NativePvmError::DuplicateAsset { .. })));
    }
}
