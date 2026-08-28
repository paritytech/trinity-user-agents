/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    ApplicationRuntime, AudioChunk, Frame, GpuBatch, InputEvent, InputEventType,
    PresentationProfile, Tri2dFrame, MAX_ASSET_BYTES, MAX_ASSET_FILES, MAX_ASSET_FILE_BYTES,
    MAX_PROGRAM_BYTES,
};
use anyhow::{anyhow, Result};
use polkavm::BackendKind;
use std::cell::RefCell;
use std::collections::HashMap;

const MAX_ASSET_NAME_BYTES: usize = 1_024;
const MAX_STAGING_BYTES: usize = MAX_ASSET_FILE_BYTES + MAX_ASSET_NAME_BYTES;

struct Launch {
    program: Vec<u8>,
    assets: HashMap<String, Vec<u8>>,
    asset_bytes: usize,
    max_gas_per_update: u64,
    audio_enabled: bool,
    presentation: PresentationProfile,
}

enum Phase {
    Empty,
    Building(Launch),
    Running(ApplicationRuntime),
}

struct BrowserHost {
    phase: Phase,
    staging: Vec<u8>,
    frame: Option<Frame>,
    tri2d: Option<Tri2dFrame>,
    gpu_batch: Option<GpuBatch>,
    audio: Option<AudioChunk>,
    log: Option<String>,
    save: Option<Vec<u8>>,
    translation: Vec<u8>,
    error: String,
}

impl BrowserHost {
    fn new() -> Self {
        Self {
            phase: Phase::Empty,
            staging: Vec::new(),
            frame: None,
            tri2d: None,
            gpu_batch: None,
            audio: None,
            log: None,
            save: None,
            translation: Vec::new(),
            error: String::new(),
        }
    }

    fn running(&mut self) -> Result<&mut ApplicationRuntime> {
        match &mut self.phase {
            Phase::Running(runtime) => Ok(runtime),
            _ => Err(anyhow!("PolkaVM browser runtime is not running")),
        }
    }

    fn clear_outputs(&mut self) {
        self.frame = None;
        self.tri2d = None;
        self.gpu_batch = None;
        self.audio = None;
        self.log = None;
        self.save = None;
    }
}

thread_local! {
    static HOST: RefCell<BrowserHost> = RefCell::new(BrowserHost::new());
}

fn status(operation: impl FnOnce(&mut BrowserHost) -> Result<()>) -> u32 {
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.error.clear();
        match operation(&mut host) {
            Ok(()) => 0,
            Err(error) => {
                host.error = format!("{error:#}");
                1
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_abi_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn pvm_browser_reset() {
    HOST.with(|host| *host.borrow_mut() = BrowserHost::new());
}

#[no_mangle]
pub extern "C" fn pvm_browser_staging_reserve(length: u32) -> u32 {
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.error.clear();
        let length = length as usize;
        if length == 0 || length > MAX_STAGING_BYTES {
            host.error = format!("invalid PolkaVM browser staging length {length}");
            host.staging.clear();
            return 0;
        }
        host.staging = vec![0; length];
        host.staging.as_mut_ptr() as usize as u32
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_translate_staged() -> u32 {
    status(|host| {
        host.translation = crate::wasm_codegen::translate(&host.staging)?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_translation_pointer() -> u32 {
    HOST.with(|host| host.borrow().translation.as_ptr() as usize as u32)
}

#[no_mangle]
pub extern "C" fn pvm_browser_translation_length() -> u32 {
    HOST.with(|host| host.borrow().translation.len() as u32)
}

fn launch_begin(max_gas_per_update: u32, audio_enabled: u32, presentation: u32) -> u32 {
    status(|host| {
        if !matches!(host.phase, Phase::Empty) {
            return Err(anyhow!("PolkaVM browser launch is already active"));
        }
        let program = std::mem::take(&mut host.staging);
        if program.is_empty() || program.len() > MAX_PROGRAM_BYTES {
            return Err(anyhow!(
                "PolkaVM browser program must contain 1..={MAX_PROGRAM_BYTES} bytes"
            ));
        }
        if max_gas_per_update == 0 {
            return Err(anyhow!("PolkaVM browser gas budget must be nonzero"));
        }
        if audio_enabled > 1 {
            return Err(anyhow!("invalid PolkaVM browser audio capability"));
        }
        let presentation = match presentation {
            0 => PresentationProfile::Framebuffer,
            1 => PresentationProfile::Tri2d,
            2 => PresentationProfile::WebGpuRaster,
            _ => return Err(anyhow!("invalid PolkaVM browser presentation profile")),
        };
        host.phase = Phase::Building(Launch {
            program,
            assets: HashMap::new(),
            asset_bytes: 0,
            max_gas_per_update: max_gas_per_update.into(),
            audio_enabled: audio_enabled == 1,
            presentation,
        });
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_launch_begin(max_gas_per_update: u32, audio_enabled: u32) -> u32 {
    launch_begin(max_gas_per_update, audio_enabled, 0)
}

#[no_mangle]
pub extern "C" fn pvm_browser_launch_begin_v2(
    max_gas_per_update: u32,
    audio_enabled: u32,
    presentation: u32,
) -> u32 {
    launch_begin(max_gas_per_update, audio_enabled, presentation)
}

#[no_mangle]
pub extern "C" fn pvm_browser_launch_add_asset(path_length: u32) -> u32 {
    status(|host| {
        let bytes = std::mem::take(&mut host.staging);
        let path_length = path_length as usize;
        if path_length == 0 || path_length > MAX_ASSET_NAME_BYTES || path_length >= bytes.len() {
            return Err(anyhow!("invalid PolkaVM browser asset path length"));
        }
        let path = std::str::from_utf8(&bytes[..path_length])
            .map_err(|_| anyhow!("PolkaVM browser asset path is not UTF-8"))?;
        if path.contains('\0') || path.contains('\\') {
            return Err(anyhow!("invalid PolkaVM browser asset path"));
        }
        let data = &bytes[path_length..];
        if data.len() > MAX_ASSET_FILE_BYTES {
            return Err(anyhow!("PolkaVM browser asset exceeds size limit"));
        }
        let Phase::Building(launch) = &mut host.phase else {
            return Err(anyhow!("PolkaVM browser launch is not accepting assets"));
        };
        if launch.assets.len() == MAX_ASSET_FILES {
            return Err(anyhow!("PolkaVM browser launch exceeds asset count limit"));
        }
        let asset_bytes = launch
            .asset_bytes
            .checked_add(data.len())
            .ok_or_else(|| anyhow!("PolkaVM browser asset size overflow"))?;
        if asset_bytes > MAX_ASSET_BYTES {
            return Err(anyhow!("PolkaVM browser launch exceeds asset byte limit"));
        }
        if launch.assets.contains_key(path) {
            return Err(anyhow!("duplicate PolkaVM browser asset {path}"));
        }
        launch.assets.insert(path.to_owned(), data.to_vec());
        launch.asset_bytes = asset_bytes;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_launch_start() -> u32 {
    status(|host| {
        let phase = std::mem::replace(&mut host.phase, Phase::Empty);
        let Phase::Building(launch) = phase else {
            host.phase = phase;
            return Err(anyhow!("PolkaVM browser launch is not ready"));
        };
        let runtime = ApplicationRuntime::new_with_backend(
            &launch.program,
            launch.assets,
            launch.presentation,
            launch.audio_enabled,
            launch.max_gas_per_update,
            BackendKind::Interpreter,
        )?;
        host.phase = Phase::Running(runtime);
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_set_gpu_capabilities() -> u32 {
    status(|host| {
        let bytes = std::mem::take(&mut host.staging);
        host.running()?.set_gpu_capabilities(bytes)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_send_gpu_event() -> u32 {
    status(|host| {
        let bytes = std::mem::take(&mut host.staging);
        host.running()?.send_gpu_event(bytes)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_init() -> u32 {
    status(|host| host.running()?.init())
}

#[no_mangle]
pub extern "C" fn pvm_browser_update(time_ms: f64) -> u32 {
    status(|host| {
        if !time_ms.is_finite() || time_ms < 0.0 {
            return Err(anyhow!("invalid PolkaVM browser timestamp"));
        }
        let runtime = host.running()?;
        runtime.set_time_ms(time_ms.min(u64::MAX as f64) as u64);
        runtime.update()
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_send_input(event_type: u32, code: u32, x: u32, y: u32) -> u32 {
    status(|host| {
        let event_type = match event_type {
            1 => InputEventType::KeyDown,
            2 => InputEventType::KeyUp,
            3 => InputEventType::ButtonDown,
            4 => InputEventType::ButtonUp,
            5 => InputEventType::PointerMove,
            6 => InputEventType::PointerDelta,
            7 => InputEventType::SurfaceMetrics,
            _ => return Err(anyhow!("invalid PolkaVM browser input event type")),
        };
        let code = u8::try_from(code).map_err(|_| anyhow!("input code exceeds u8"))?;
        let x = u16::try_from(x).map_err(|_| anyhow!("input x exceeds u16"))?;
        let y = u16::try_from(y).map_err(|_| anyhow!("input y exceeds u16"))?;
        host.running()?.send_input(InputEvent {
            event_type,
            code,
            x,
            y,
        });
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_take_frame() -> u32 {
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.frame = match &mut host.phase {
            Phase::Running(runtime) => runtime.take_frame(),
            _ => None,
        };
        u32::from(host.frame.is_some())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_frame_width() -> u32 {
    HOST.with(|host| host.borrow().frame.as_ref().map_or(0, |frame| frame.width))
}

#[no_mangle]
pub extern "C" fn pvm_browser_frame_height() -> u32 {
    HOST.with(|host| host.borrow().frame.as_ref().map_or(0, |frame| frame.height))
}

#[no_mangle]
pub extern "C" fn pvm_browser_frame_pointer() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .frame
            .as_ref()
            .map_or(0, |frame| frame.argb.as_ptr() as usize as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_frame_length() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .frame
            .as_ref()
            .map_or(0, |frame| frame.argb.len() as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_take_tri2d() -> u32 {
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.tri2d = match &mut host.phase {
            Phase::Running(runtime) => runtime.take_tri2d(),
            _ => None,
        };
        u32::from(host.tri2d.is_some())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_tri2d_pointer() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .tri2d
            .as_ref()
            .map_or(0, |frame| frame.bytes.as_ptr() as usize as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_tri2d_length() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .tri2d
            .as_ref()
            .map_or(0, |frame| frame.bytes.len() as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_take_gpu_batch() -> u32 {
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.gpu_batch = match &mut host.phase {
            Phase::Running(runtime) => runtime.take_gpu_batch(),
            _ => None,
        };
        u32::from(host.gpu_batch.is_some())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_gpu_batch_pointer() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .gpu_batch
            .as_ref()
            .map_or(0, |batch| batch.bytes.as_ptr() as usize as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_gpu_batch_length() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .gpu_batch
            .as_ref()
            .map_or(0, |batch| batch.bytes.len() as u32)
    })
}
#[no_mangle]
pub extern "C" fn pvm_browser_take_audio() -> u32 {
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.audio = match &mut host.phase {
            Phase::Running(runtime) => runtime.take_audio(),
            _ => None,
        };
        u32::from(host.audio.is_some())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_audio_pointer() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .audio
            .as_ref()
            .map_or(0, |audio| audio.samples.as_ptr() as usize as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_audio_length() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .audio
            .as_ref()
            .map_or(0, |audio| audio.samples.len() as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_audio_sample_rate() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .audio
            .as_ref()
            .map_or(0, |audio| audio.sample_rate)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_audio_channels() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .audio
            .as_ref()
            .map_or(0, |audio| audio.channels)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_take_log() -> u32 {
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.log = match &mut host.phase {
            Phase::Running(runtime) => runtime.take_log(),
            _ => None,
        };
        u32::from(host.log.is_some())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_log_pointer() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .log
            .as_ref()
            .map_or(0, |log| log.as_ptr() as usize as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_log_length() -> u32 {
    HOST.with(|host| host.borrow().log.as_ref().map_or(0, |log| log.len() as u32))
}

#[no_mangle]
pub extern "C" fn pvm_browser_take_save() -> u32 {
    HOST.with(|host| {
        let mut host = host.borrow_mut();
        host.save = match &mut host.phase {
            Phase::Running(runtime) => runtime.take_save(),
            _ => None,
        };
        u32::from(host.save.is_some())
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_save_pointer() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .save
            .as_ref()
            .map_or(0, |save| save.as_ptr() as usize as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_save_length() -> u32 {
    HOST.with(|host| {
        host.borrow()
            .save
            .as_ref()
            .map_or(0, |save| save.len() as u32)
    })
}

#[no_mangle]
pub extern "C" fn pvm_browser_error_pointer() -> u32 {
    HOST.with(|host| host.borrow().error.as_ptr() as usize as u32)
}

#[no_mangle]
pub extern "C" fn pvm_browser_error_length() -> u32 {
    HOST.with(|host| host.borrow().error.len() as u32)
}

#[no_mangle]
pub extern "C" fn pvm_browser_clear_outputs() {
    HOST.with(|host| host.borrow_mut().clear_outputs());
}
