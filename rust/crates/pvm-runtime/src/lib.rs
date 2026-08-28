/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(target_arch = "wasm32")]
extern crate polkavm_wasm as polkavm;

mod application;
mod corevm;
pub use pvm_gpu_wire as gpu_wire;
mod manifest;
#[cfg(all(not(target_arch = "wasm32"), feature = "ffi"))]
mod native_ffi;
mod quake_keys;
mod tri2d;
#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(any(target_arch = "wasm32", test))]
mod wasm_codegen;

#[cfg(all(not(target_arch = "wasm32"), feature = "ffi"))]
pub use native_ffi::*;

#[cfg(all(not(target_arch = "wasm32"), feature = "ffi"))]
uniffi::setup_scaffolding!();

pub use application::ApplicationRuntime;
pub use manifest::AppDescriptor;

use anyhow::{anyhow, Context, Result};
pub use polkavm::BackendKind;
use polkavm::{CallError, Config, Engine, Instance, Linker, Module, ProgramBlob};
use std::collections::{HashMap, VecDeque};
use std::mem::size_of;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
pub use tri2d::{
    Tri2dFrame, MAX_TRI2D_BYTES, MAX_TRI2D_COMMANDS, MAX_TRI2D_DRAWS, MAX_TRI2D_INDICES,
    MAX_TRI2D_SURFACE_SIZE, MAX_TRI2D_TEXTURES, MAX_TRI2D_TEXTURE_BYTES, MAX_TRI2D_TEXTURE_SIZE,
    MAX_TRI2D_VERTICES, TRI2D_HEADER_BYTES, TRI2D_MAGIC, TRI2D_VERSION,
};

pub const ABI_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationProfile {
    Framebuffer,
    Tri2d,
    WebGpuRaster,
}

impl PresentationProfile {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "framebuffer" => Ok(Self::Framebuffer),
            "tri2d" => Ok(Self::Tri2d),
            "webgpu-raster" => Ok(Self::WebGpuRaster),
            _ => Err(anyhow!("unsupported presentation profile {value}")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Framebuffer => "framebuffer",
            Self::Tri2d => "tri2d",
            Self::WebGpuRaster => "webgpu-raster",
        }
    }
}
pub const BYTES_PER_PIXEL: usize = 4;
pub const MAX_PROGRAM_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_GUEST_READ: usize = MAX_FRAME_BYTES;
pub const MAX_GUEST_RW_DATA_BYTES: u32 = 64 * 1024 * 1024;
pub const MAX_GUEST_STACK_BYTES: u32 = 16 * 1024 * 1024;
pub const MAX_GUEST_HEAP_BYTES: u32 = 128 * 1024 * 1024;
pub const MAX_ASSET_FILES: usize = 2_048;
pub const MAX_ASSET_FILE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_ASSET_BYTES: usize = 128 * 1024 * 1024;
pub const INPUT_EVENT_BYTES: usize = 8;
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;
pub const AUDIO_CHANNELS: u32 = 2;
pub const MAX_AUDIO_SAMPLES_PER_CALL: usize = AUDIO_SAMPLE_RATE as usize * AUDIO_CHANNELS as usize;
const MAX_ASSET_NAME_BYTES: usize = 1_024;
const MAX_ASSET_READ_BYTES: usize = 16 * 1024 * 1024;
const MAX_HOSTCALL_BYTES_PER_TICK: usize = 32 * 1024 * 1024;
const MAX_HOSTCALLS_PER_INIT: u32 = 131_072;
const MAX_HOSTCALLS_PER_UPDATE: u32 = 8_192;
const MAX_SLEEP_MS_PER_INIT: u32 = 100;
const MAX_SLEEP_MS_PER_UPDATE: u32 = 50;
const MAX_QUEUED_AUDIO_SAMPLES: usize = AUDIO_SAMPLE_RATE as usize * AUDIO_CHANNELS as usize * 2;
const MAX_QUEUED_INPUT_EVENTS: usize = 4_096;
const MAX_SAVE_BYTES: usize = 1024 * 1024;
const MAX_LOG_BYTES: usize = 4 * 1024;
const MAX_QUEUED_LOGS: usize = 64;
const MAX_QUEUED_GPU_BATCHES: usize = 4;
const MAX_QUEUED_GPU_EVENTS: usize = 256;
const MAX_GPU_SUBMITS_PER_TICK: u32 = 8;
const MAX_GPU_UPLOAD_BYTES_PER_TICK: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum InputEventType {
    KeyDown = 1,
    KeyUp = 2,
    ButtonDown = 3,
    ButtonUp = 4,
    PointerMove = 5,
    PointerDelta = 6,
    SurfaceMetrics = 7,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEvent {
    pub event_type: InputEventType,
    pub code: u8,
    pub x: u16,
    pub y: u16,
}

impl InputEvent {
    fn encode(self) -> [u8; INPUT_EVENT_BYTES] {
        let x = self.x.to_le_bytes();
        let y = self.y.to_le_bytes();
        [
            self.event_type as u8,
            self.code,
            x[0],
            x[1],
            y[0],
            y[1],
            0,
            0,
        ]
    }
}

#[derive(Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Packed 0xAARRGGBB pixels. On little-endian guests these bytes are BGRA.
    pub argb: Vec<u8>,
}

#[derive(Debug)]
pub struct AudioChunk {
    /// Interleaved little-endian signed 16-bit samples.
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u32,
}

#[derive(Debug)]
pub struct GpuBatch {
    pub bytes: Vec<u8>,
}

struct HostClock {
    #[cfg(not(target_arch = "wasm32"))]
    started: Instant,
    #[cfg(target_arch = "wasm32")]
    now_ms: u64,
}

impl HostClock {
    fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            started: Instant::now(),
            #[cfg(target_arch = "wasm32")]
            now_ms: 0,
        }
    }

    fn elapsed_ms(&self) -> u64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started.elapsed().as_millis() as u64
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.now_ms
        }
    }

    fn sleep_ms(&mut self, duration_ms: u32) {
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::sleep(Duration::from_millis(duration_ms.into()));
        #[cfg(target_arch = "wasm32")]
        {
            self.now_ms = self.now_ms.saturating_add(duration_ms.into());
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn set_time_ms(&mut self, time_ms: u64) {
        self.now_ms = self.now_ms.max(time_ms);
    }
}

pub(crate) fn preferred_backend() -> BackendKind {
    #[cfg(any(target_arch = "wasm32", target_os = "ios", target_os = "android"))]
    {
        BackendKind::Interpreter
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios", target_os = "android")))]
    {
        BackendKind::Compiler
    }
}

struct HostState {
    frame: Option<Frame>,
    tri2d: Option<Tri2dFrame>,
    tri2d_state: tri2d::Tri2dState,
    audio_enabled: bool,
    presentation: PresentationProfile,
    tri2d_submitted: bool,
    audio: VecDeque<AudioChunk>,
    audio_samples: usize,
    input: VecDeque<InputEvent>,
    assets: HashMap<String, Vec<u8>>,
    clock: HostClock,
    logs: VecDeque<String>,
    save: Option<Vec<u8>>,
    hostcall_bytes_remaining: usize,
    hostcalls_remaining: u32,
    sleep_ms_remaining: u32,
    gpu_capabilities: Option<Vec<u8>>,
    gpu_batches: VecDeque<GpuBatch>,
    gpu_events: VecDeque<Vec<u8>>,
    gpu_last_sequence: u64,
    gpu_submits_remaining: u32,
    gpu_upload_bytes_remaining: usize,
}

impl HostState {
    fn new(
        assets: HashMap<String, Vec<u8>>,
        presentation: PresentationProfile,
        audio_enabled: bool,
    ) -> Self {
        Self {
            frame: None,
            tri2d: None,
            presentation,
            audio_enabled,
            tri2d_state: tri2d::Tri2dState::default(),
            tri2d_submitted: false,
            audio: VecDeque::new(),
            audio_samples: 0,
            input: VecDeque::new(),
            assets,
            clock: HostClock::new(),
            logs: VecDeque::new(),
            save: None,
            hostcall_bytes_remaining: 0,
            hostcalls_remaining: 0,
            sleep_ms_remaining: 0,
            gpu_capabilities: None,
            gpu_batches: VecDeque::new(),
            gpu_events: VecDeque::new(),
            gpu_last_sequence: 0,
            gpu_submits_remaining: 0,
            gpu_upload_bytes_remaining: 0,
        }
    }

    fn reset_hostcall_budget(&mut self, max_hostcalls: u32, max_sleep_ms: u32) {
        self.hostcall_bytes_remaining = MAX_HOSTCALL_BYTES_PER_TICK;
        self.hostcalls_remaining = max_hostcalls;
        self.sleep_ms_remaining = max_sleep_ms;
        self.tri2d_submitted = false;
        self.gpu_submits_remaining = MAX_GPU_SUBMITS_PER_TICK;
        self.gpu_upload_bytes_remaining = MAX_GPU_UPLOAD_BYTES_PER_TICK;
    }

    fn charge_hostcall(&mut self, bytes: usize) -> Result<()> {
        if self.hostcalls_remaining == 0 {
            return Err(anyhow!("guest exceeded host-call count budget"));
        }
        self.hostcalls_remaining -= 1;
        self.charge_hostcall_bytes(bytes)
    }

    fn charge_hostcall_bytes(&mut self, bytes: usize) -> Result<()> {
        if bytes > self.hostcall_bytes_remaining {
            return Err(anyhow!("guest exceeded host-call byte budget"));
        }
        self.hostcall_bytes_remaining -= bytes;
        Ok(())
    }

    fn queue_input(&mut self, event: InputEvent) {
        if event.event_type == InputEventType::SurfaceMetrics {
            if let Some(position) = self
                .input
                .iter()
                .rposition(|queued| queued.event_type == InputEventType::SurfaceMetrics)
            {
                self.input.remove(position);
            }
        } else if event.event_type == InputEventType::PointerMove
            && self
                .input
                .back()
                .is_some_and(|queued| queued.event_type == InputEventType::PointerMove)
        {
            self.input.pop_back();
        }
        if self.input.len() == MAX_QUEUED_INPUT_EVENTS {
            let discardable = self
                .input
                .iter()
                .position(|queued| queued.event_type == InputEventType::PointerMove)
                .or_else(|| {
                    self.input
                        .iter()
                        .position(|queued| queued.event_type != InputEventType::SurfaceMetrics)
                });
            if let Some(position) = discardable {
                self.input.remove(position);
            } else {
                self.input.pop_front();
            }
        }
        self.input.push_back(event);
    }
}

pub struct Runtime {
    instance: Instance<HostState, anyhow::Error>,
    state: HostState,
    max_gas_per_update: u64,
    last_gas_used: u64,
    backend: polkavm::BackendKind,
}

impl Runtime {
    pub fn new(
        program: &[u8],
        assets: HashMap<String, Vec<u8>>,
        presentation: PresentationProfile,
        audio_enabled: bool,
        max_gas_per_update: u64,
    ) -> Result<Self> {
        Self::new_with_backend(
            program,
            assets,
            presentation,
            audio_enabled,
            max_gas_per_update,
            preferred_backend(),
        )
    }

    pub fn new_with_backend(
        program: &[u8],
        assets: HashMap<String, Vec<u8>>,
        presentation: PresentationProfile,
        audio_enabled: bool,
        max_gas_per_update: u64,
        backend: BackendKind,
    ) -> Result<Self> {
        let blob = ProgramBlob::parse(program.into()).context("parse PolkaVM program")?;
        if blob.rw_data_size() > MAX_GUEST_RW_DATA_BYTES {
            return Err(anyhow!(
                "guest read-write data exceeds {MAX_GUEST_RW_DATA_BYTES} bytes"
            ));
        }
        if blob.stack_size() > MAX_GUEST_STACK_BYTES {
            return Err(anyhow!("guest stack exceeds {MAX_GUEST_STACK_BYTES} bytes"));
        }
        let mut engine_config = Config::new();
        // macOS requires PolkaVM's experimental generic sandbox for native
        // recompilation. Keep sandboxing enabled while opting into that boundary.
        engine_config.set_backend(Some(backend));
        engine_config.set_sandboxing_enabled(true);
        #[cfg(target_os = "macos")]
        {
            engine_config.set_sandbox(Some(polkavm::SandboxKind::Generic));
            engine_config.set_allow_experimental(true);
        }
        let engine = Engine::new(&engine_config).context("create PolkaVM engine")?;
        let backend = engine.backend();

        let mut module_config = polkavm::ModuleConfig::default();
        module_config.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
        #[cfg(not(target_arch = "wasm32"))]
        module_config.set_max_heap_size(Some(MAX_GUEST_HEAP_BYTES));
        #[cfg(target_os = "macos")]
        module_config.set_page_size(16_384);

        let module =
            Module::from_blob(&engine, &module_config, blob).context("compile PolkaVM module")?;
        let mut linker: Linker<HostState, anyhow::Error> = Linker::new();

        linker
            .define_typed(
                "host_present_frame",
                |caller: polkavm::Caller<'_, HostState>,
                 pointer: u32,
                 width: u32,
                 height: u32,
                 stride: u32|
                 -> Result<u32> {
                    let row_bytes = width
                        .checked_mul(BYTES_PER_PIXEL as u32)
                        .ok_or_else(|| anyhow!("frame row size overflow"))?;
                    if stride != row_bytes {
                        return Ok(1);
                    }
                    let byte_len = row_bytes
                        .checked_mul(height)
                        .ok_or_else(|| anyhow!("frame size overflow"))?
                        as usize;
                    if byte_len == 0 || byte_len > MAX_FRAME_BYTES {
                        return Ok(1);
                    }
                    caller.user_data.charge_hostcall(byte_len)?;
                    if caller.user_data.presentation != PresentationProfile::Framebuffer {
                        return Ok(3);
                    }
                    let argb = read_guest_memory(caller.instance, pointer, byte_len)?;
                    caller.user_data.frame = Some(Frame {
                        width,
                        height,
                        argb,
                    });
                    Ok(0)
                },
            )
            .context("define host_present_frame")?;

        linker
            .define_typed(
                "host_tri2d_submit",
                |caller: polkavm::Caller<'_, HostState>,
                 pointer: u32,
                 length: u32|
                 -> Result<u32> {
                    let length = length as usize;
                    if length == 0 || length > MAX_TRI2D_BYTES {
                        return Ok(1);
                    }
                    caller.user_data.charge_hostcall(length)?;
                    if caller.user_data.presentation != PresentationProfile::Tri2d {
                        return Ok(3);
                    }
                    if caller.user_data.tri2d_submitted {
                        return Ok(2);
                    }
                    let bytes = read_guest_memory(caller.instance, pointer, length)?;
                    let Ok((next_state, metadata)) =
                        tri2d::validate_tri2d(&bytes, &caller.user_data.tri2d_state)
                    else {
                        return Ok(1);
                    };
                    caller.user_data.tri2d_state = next_state;
                    caller.user_data.tri2d_submitted = true;
                    caller.user_data.tri2d = Some(Tri2dFrame {
                        width: metadata.width,
                        height: metadata.height,
                        draw_count: metadata.draw_count,
                        vertex_count: metadata.vertex_count,
                        index_count: metadata.index_count,
                        bytes,
                    });
                    Ok(0)
                },
            )
            .context("define host_tri2d_submit")?;

        linker
            .define_typed(
                "epoca_gpu_capabilities",
                |caller: polkavm::Caller<'_, HostState>,
                 pointer: u32,
                 capacity: u32|
                 -> Result<i32> {
                    caller.user_data.charge_hostcall(0)?;
                    if caller.user_data.presentation != PresentationProfile::WebGpuRaster {
                        return Ok(gpu_wire::GPU_ERROR_INVALID_STATE);
                    }
                    let Some(required_len) =
                        caller.user_data.gpu_capabilities.as_ref().map(Vec::len)
                    else {
                        return Ok(0);
                    };
                    let required = i32::try_from(required_len)
                        .map_err(|_| anyhow!("GPU capabilities length overflow"))?;
                    if (capacity as usize) < required_len {
                        return Ok(-required);
                    }
                    caller.user_data.charge_hostcall_bytes(required_len)?;
                    let capabilities = caller.user_data.gpu_capabilities.as_ref().unwrap();
                    if caller.instance.write_memory(pointer, capabilities).is_err() {
                        return Ok(gpu_wire::GPU_ERROR_INVALID_GUEST_RANGE);
                    }
                    Ok(required)
                },
            )
            .context("define epoca_gpu_capabilities")?;

        linker
            .define_typed(
                "epoca_gpu_submit",
                |caller: polkavm::Caller<'_, HostState>,
                 pointer: u32,
                 length: u32|
                 -> Result<i32> {
                    let length = length as usize;
                    caller.user_data.charge_hostcall(length)?;
                    if caller.user_data.presentation != PresentationProfile::WebGpuRaster {
                        return Ok(gpu_wire::GPU_ERROR_INVALID_STATE);
                    }
                    if length == 0 || length > gpu_wire::MAX_GPU_BATCH_BYTES {
                        return Ok(gpu_wire::GPU_ERROR_MALFORMED_BATCH);
                    }
                    if caller.user_data.gpu_capabilities.is_none() {
                        return Ok(gpu_wire::GPU_ERROR_INVALID_STATE);
                    }
                    if caller.user_data.gpu_batches.len() == MAX_QUEUED_GPU_BATCHES {
                        return Ok(gpu_wire::GPU_SUBMIT_BUSY);
                    }
                    if caller.user_data.gpu_submits_remaining == 0 {
                        return Ok(gpu_wire::GPU_ERROR_QUOTA_EXCEEDED);
                    }
                    let Ok(bytes) = read_guest_memory(caller.instance, pointer, length) else {
                        return Ok(gpu_wire::GPU_ERROR_INVALID_GUEST_RANGE);
                    };
                    let Ok(batch) = gpu_wire::decode_gpu_batch(&bytes) else {
                        return Ok(gpu_wire::GPU_ERROR_MALFORMED_BATCH);
                    };
                    if batch.sequence() <= caller.user_data.gpu_last_sequence {
                        return Ok(gpu_wire::GPU_ERROR_INVALID_STATE);
                    }
                    let Some(upload_bytes) = gpu_inline_upload_bytes(&batch) else {
                        return Ok(gpu_wire::GPU_ERROR_MALFORMED_BATCH);
                    };
                    if upload_bytes > caller.user_data.gpu_upload_bytes_remaining {
                        return Ok(gpu_wire::GPU_ERROR_QUOTA_EXCEEDED);
                    }
                    caller.user_data.gpu_submits_remaining -= 1;
                    caller.user_data.gpu_upload_bytes_remaining -= upload_bytes;
                    caller.user_data.gpu_last_sequence = batch.sequence();
                    caller.user_data.gpu_batches.push_back(GpuBatch { bytes });
                    Ok(gpu_wire::GPU_SUBMIT_ACCEPTED)
                },
            )
            .context("define epoca_gpu_submit")?;

        linker
            .define_typed(
                "epoca_gpu_receive",
                |caller: polkavm::Caller<'_, HostState>,
                 pointer: u32,
                 capacity: u32|
                 -> Result<i32> {
                    caller.user_data.charge_hostcall(0)?;
                    if caller.user_data.presentation != PresentationProfile::WebGpuRaster {
                        return Ok(gpu_wire::GPU_ERROR_INVALID_STATE);
                    }
                    let Some(required_len) = caller.user_data.gpu_events.front().map(Vec::len)
                    else {
                        return Ok(0);
                    };
                    let required = i32::try_from(required_len)
                        .map_err(|_| anyhow!("GPU event length overflow"))?;
                    if (capacity as usize) < required_len {
                        return Ok(-required);
                    }
                    caller.user_data.charge_hostcall_bytes(required_len)?;
                    let event = caller.user_data.gpu_events.front().unwrap();
                    if caller.instance.write_memory(pointer, event).is_err() {
                        return Ok(gpu_wire::GPU_ERROR_INVALID_GUEST_RANGE);
                    }
                    caller.user_data.gpu_events.pop_front();
                    Ok(required)
                },
            )
            .context("define epoca_gpu_receive")?;

        linker
            .define_typed(
                "host_poll_input",
                |caller: polkavm::Caller<'_, HostState>,
                 pointer: u32,
                 capacity: u32|
                 -> Result<u32> {
                    let event_count =
                        (capacity as usize / INPUT_EVENT_BYTES).min(caller.user_data.input.len());
                    let byte_count = event_count
                        .checked_mul(INPUT_EVENT_BYTES)
                        .ok_or_else(|| anyhow!("input byte count overflow"))?;
                    caller.user_data.charge_hostcall(byte_count)?;
                    let mut written = 0u32;
                    for _ in 0..event_count {
                        let Some(event) = caller.user_data.input.pop_front() else {
                            break;
                        };
                        let destination = pointer
                            .checked_add(written)
                            .ok_or_else(|| anyhow!("guest input destination overflow"))?;
                        caller
                            .instance
                            .write_memory(destination, &event.encode())
                            .map_err(|error| anyhow!("write guest input: {error:?}"))?;
                        written += INPUT_EVENT_BYTES as u32;
                    }
                    Ok(written)
                },
            )
            .context("define host_poll_input")?;

        linker
            .define_typed(
                "host_time_ms",
                |caller: polkavm::Caller<'_, HostState>| -> Result<u64> {
                    caller.user_data.charge_hostcall(0)?;
                    Ok(caller.user_data.clock.elapsed_ms())
                },
            )
            .context("define host_time_ms")?;

        linker
            .define_typed(
                "host_sleep_ms",
                |caller: polkavm::Caller<'_, HostState>, duration_ms: u32| -> Result<()> {
                    caller.user_data.charge_hostcall(0)?;
                    let duration_ms = duration_ms.min(caller.user_data.sleep_ms_remaining);
                    caller.user_data.sleep_ms_remaining -= duration_ms;
                    caller.user_data.clock.sleep_ms(duration_ms);
                    Ok(())
                },
            )
            .context("define host_sleep_ms")?;

        linker
            .define_typed(
                "host_audio_submit",
                |caller: polkavm::Caller<'_, HostState>,
                 pointer: u32,
                 sample_count: u32|
                 -> Result<u32> {
                    let sample_count = sample_count as usize;
                    if sample_count == 0
                        || sample_count % AUDIO_CHANNELS as usize != 0
                        || sample_count > MAX_AUDIO_SAMPLES_PER_CALL
                        || caller.user_data.audio_samples + sample_count > MAX_QUEUED_AUDIO_SAMPLES
                    {
                        return Ok(1);
                    }
                    let byte_count = sample_count
                        .checked_mul(size_of::<i16>())
                        .ok_or_else(|| anyhow!("audio byte count overflow"))?;
                    caller.user_data.charge_hostcall(byte_count)?;
                    if !caller.user_data.audio_enabled {
                        return Ok(3);
                    }
                    let bytes = read_guest_memory(caller.instance, pointer, byte_count)?;
                    let samples = bytes
                        .chunks_exact(2)
                        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
                        .collect();
                    caller.user_data.audio_samples += sample_count;
                    caller.user_data.audio.push_back(AudioChunk {
                        samples,
                        sample_rate: AUDIO_SAMPLE_RATE,
                        channels: AUDIO_CHANNELS,
                    });
                    Ok(0)
                },
            )
            .context("define host_audio_submit")?;

        linker
            .define_typed(
                "host_asset_read",
                |caller: polkavm::Caller<'_, HostState>,
                 name_pointer: u32,
                 name_length: u32,
                 offset: u32,
                 destination: u32,
                 capacity: u32|
                 -> Result<u32> {
                    let name_length = name_length as usize;
                    caller.user_data.charge_hostcall(name_length)?;
                    if name_length == 0 || name_length > MAX_ASSET_NAME_BYTES {
                        return Ok(0);
                    }
                    let name = read_guest_memory(caller.instance, name_pointer, name_length)?;
                    let Ok(name) = std::str::from_utf8(&name) else {
                        return Ok(0);
                    };
                    let Some(asset_length) = caller.user_data.assets.get(name).map(Vec::len) else {
                        return Ok(0);
                    };
                    let offset = offset as usize;
                    if offset >= asset_length {
                        return Ok(0);
                    }
                    let length = (capacity as usize)
                        .min(asset_length - offset)
                        .min(MAX_ASSET_READ_BYTES);
                    caller.user_data.charge_hostcall_bytes(length)?;
                    let asset = &caller.user_data.assets[name];
                    caller
                        .instance
                        .write_memory(destination, &asset[offset..offset + length])
                        .map_err(|error| anyhow!("write guest asset: {error:?}"))?;
                    Ok(length as u32)
                },
            )
            .context("define host_asset_read")?;

        linker
            .define_typed(
                "host_save_submit",
                |caller: polkavm::Caller<'_, HostState>,
                 pointer: u32,
                 length: u32|
                 -> Result<u32> {
                    let length = length as usize;
                    if length == 0 || length > MAX_SAVE_BYTES {
                        return Ok(1);
                    }
                    caller.user_data.charge_hostcall(length)?;
                    caller.user_data.save =
                        Some(read_guest_memory(caller.instance, pointer, length)?);
                    Ok(0)
                },
            )
            .context("define host_save_submit")?;

        linker
            .define_typed(
                "host_log",
                |caller: polkavm::Caller<'_, HostState>, pointer: u32, length: u32| -> Result<()> {
                    let length = (length as usize).min(MAX_LOG_BYTES);
                    caller.user_data.charge_hostcall(length)?;
                    let bytes = read_guest_memory(caller.instance, pointer, length)?;
                    if caller.user_data.logs.len() == MAX_QUEUED_LOGS {
                        caller.user_data.logs.pop_front();
                    }
                    caller
                        .user_data
                        .logs
                        .push_back(String::from_utf8_lossy(&bytes).into_owned());
                    Ok(())
                },
            )
            .context("define host_log")?;

        let instance = linker
            .instantiate_pre(&module)
            .context("pre-instantiate PolkaVM module")?
            .instantiate()
            .context("instantiate PolkaVM module")?;

        Ok(Self {
            instance,
            state: HostState::new(assets, presentation, audio_enabled),
            max_gas_per_update,
            last_gas_used: 0,
            backend,
        })
    }

    pub fn init(&mut self) -> Result<()> {
        let gas = self.max_gas_per_update.min(i64::MAX as u64) as i64;
        self.instance.set_gas(gas);
        self.state
            .reset_hostcall_budget(MAX_HOSTCALLS_PER_INIT, MAX_SLEEP_MS_PER_INIT);
        let result = self
            .instance
            .call_typed_and_get_result::<(), ()>(&mut self.state, "init", ());
        self.record_gas_used(gas);
        let program_counter = self.instance.program_counter();
        map_call_result(result, "init").map_err(|error| {
            let error = if let Some(log) = self.state.logs.back() {
                error.context(format!("last guest log: {log}"))
            } else {
                error
            };
            if let Some(program_counter) = program_counter {
                error.context(format!("guest program counter: {program_counter}"))
            } else {
                error
            }
        })
    }

    pub fn update(&mut self) -> Result<()> {
        let gas = self.max_gas_per_update.min(i64::MAX as u64) as i64;
        self.instance.set_gas(gas);
        self.state
            .reset_hostcall_budget(MAX_HOSTCALLS_PER_UPDATE, MAX_SLEEP_MS_PER_UPDATE);
        let result =
            self.instance
                .call_typed_and_get_result::<(), ()>(&mut self.state, "update", ());
        self.record_gas_used(gas);
        let program_counter = self.instance.program_counter();
        map_call_result(result, "update").map_err(|error| {
            let error = if let Some(log) = self.state.logs.back() {
                error.context(format!("last guest log: {log}"))
            } else {
                error
            };
            if let Some(program_counter) = program_counter {
                error.context(format!("guest program counter: {program_counter}"))
            } else {
                error
            }
        })
    }
    fn record_gas_used(&mut self, budget: i64) {
        let remaining = self.instance.gas().max(0);
        self.last_gas_used = (budget - remaining.min(budget)) as u64;
    }

    pub fn last_gas_used(&self) -> u64 {
        self.last_gas_used
    }

    pub fn backend(&self) -> polkavm::BackendKind {
        self.backend
    }

    pub fn send_input(&mut self, event: InputEvent) {
        self.state.queue_input(event);
    }

    pub fn gpu_ready(&self) -> bool {
        self.state.presentation != PresentationProfile::WebGpuRaster
            || self.state.gpu_capabilities.is_some()
    }

    pub fn set_gpu_capabilities(&mut self, bytes: Vec<u8>) -> Result<()> {
        if self.state.presentation != PresentationProfile::WebGpuRaster {
            return Err(anyhow!("GPU capabilities sent to a non-GPU application"));
        }
        validate_gpu_capabilities(&bytes)?;
        self.state.gpu_capabilities = Some(bytes);
        Ok(())
    }

    pub fn send_gpu_event(&mut self, bytes: Vec<u8>) -> Result<()> {
        if self.state.presentation != PresentationProfile::WebGpuRaster {
            return Err(anyhow!("GPU event sent to a non-GPU application"));
        }
        validate_gpu_event(&bytes)?;
        if self.state.gpu_events.len() == MAX_QUEUED_GPU_EVENTS {
            return Err(anyhow!("GPU event queue overflow"));
        }
        self.state.gpu_events.push_back(bytes);
        Ok(())
    }

    pub fn take_gpu_batch(&mut self) -> Option<GpuBatch> {
        self.state.gpu_batches.pop_front()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn set_time_ms(&mut self, time_ms: u64) {
        self.state.clock.set_time_ms(time_ms);
    }

    pub fn take_frame(&mut self) -> Option<Frame> {
        self.state.frame.take()
    }

    pub fn take_tri2d(&mut self) -> Option<Tri2dFrame> {
        self.state.tri2d.take()
    }

    pub fn take_audio(&mut self) -> Option<AudioChunk> {
        let chunk = self.state.audio.pop_front()?;
        self.state.audio_samples -= chunk.samples.len();
        Some(chunk)
    }

    pub fn take_log(&mut self) -> Option<String> {
        self.state.logs.pop_front()
    }

    pub fn take_save(&mut self) -> Option<Vec<u8>> {
        self.state.save.take()
    }
}

fn gpu_inline_upload_bytes(batch: &gpu_wire::GpuBatch<'_>) -> Option<usize> {
    batch.commands().try_fold(0usize, |total, command| {
        let bytes = match command.opcode {
            gpu_wire::GpuOpcode::WriteBuffer => gpu_u32(command.payload, 16)? as usize,
            gpu_wire::GpuOpcode::WriteTexture => gpu_u32(command.payload, 40)? as usize,
            _ => 0,
        };
        total.checked_add(bytes)
    })
}

fn validate_gpu_capabilities(bytes: &[u8]) -> Result<()> {
    const HEADER_BYTES: usize = 56;
    const ENTRY_BYTES: usize = 16;
    if bytes.len() < HEADER_BYTES
        || bytes[..4] != gpu_wire::GPU_CAPABILITIES_MAGIC
        || gpu_u16(bytes, 4) != Some(gpu_wire::GPU_WIRE_VERSION)
        || gpu_u16(bytes, 6) != Some(0)
        || gpu_u32(bytes, 8) != Some(bytes.len() as u32)
        || !matches!(gpu_u16(bytes, 12), Some(1..=4))
        || gpu_u16(bytes, 14) != Some(0)
        || gpu_u32(bytes, 16) == Some(0)
        || gpu_u32(bytes, 20) == Some(0)
        || gpu_u32(bytes, 24) == Some(0)
        || gpu_u32(bytes, 28) == Some(0)
        || gpu_u32(bytes, 36) == Some(0)
        || gpu_u32(bytes, 40) == Some(0)
        || gpu_u32(bytes, 48) != Some(0)
        || gpu_u32(bytes, 52) != Some(0)
    {
        return Err(anyhow!("invalid GPU capabilities record"));
    }
    let scale = f32::from_bits(gpu_u32(bytes, 32).unwrap());
    if !scale.is_finite() || scale <= 0.0 {
        return Err(anyhow!("invalid GPU capabilities scale"));
    }
    let count = gpu_u32(bytes, 44).unwrap() as usize;
    if count > 12 || bytes.len() != HEADER_BYTES + count * ENTRY_BYTES {
        return Err(anyhow!("invalid GPU capabilities limit table"));
    }
    let mut previous = 0;
    for entry in bytes[HEADER_BYTES..].chunks_exact(ENTRY_BYTES) {
        let key = gpu_u16(entry, 0).unwrap();
        if key <= previous
            || key > 12
            || gpu_u16(entry, 2) != Some(0)
            || gpu_u64(entry, 4) == Some(0)
            || gpu_u32(entry, 12) != Some(0)
        {
            return Err(anyhow!("invalid GPU capability limit entry"));
        }
        previous = key;
    }
    Ok(())
}

fn validate_gpu_event(bytes: &[u8]) -> Result<()> {
    if bytes.len() < gpu_wire::GPU_EVENT_HEADER_BYTES
        || bytes.len() > gpu_wire::MAX_GPU_EVENT_BYTES
        || bytes[..4] != gpu_wire::GPU_EVENT_MAGIC
        || gpu_u16(bytes, 4) != Some(gpu_wire::GPU_WIRE_VERSION)
        || !matches!(gpu_u16(bytes, 6), Some(1..=7))
        || gpu_u32(bytes, 8) != Some(bytes.len() as u32)
        || gpu_u32(bytes, 12) != Some(0)
    {
        return Err(anyhow!("invalid GPU event envelope"));
    }
    let event_type = gpu_u16(bytes, 6).unwrap();
    let payload = &bytes[gpu_wire::GPU_EVENT_HEADER_BYTES..];
    match event_type {
        1 | 3 => validate_gpu_text(payload, 8, 12, 16),
        2 => {
            if payload.len() < 32
                || gpu_u16(payload, 6).is_none_or(|flags| flags > 1)
                || gpu_u32(payload, 28) != Some(0)
            {
                return Err(anyhow!("invalid GPU shader diagnostic"));
            }
            validate_gpu_text(payload, 24, 6, 32)
        }
        4 | 7 => validate_gpu_text(payload, 4, 8, 12),
        5 if payload.is_empty() => Ok(()),
        6 => {
            if payload.len() != 28
                || gpu_u32(payload, 0) == Some(0)
                || gpu_u32(payload, 4) == Some(0)
                || gpu_u32(payload, 8) == Some(0)
                || gpu_u32(payload, 12) == Some(0)
                || gpu_u32(payload, 16) == Some(0)
                || !f32::from_bits(gpu_u32(payload, 20).unwrap()).is_finite()
                || f32::from_bits(gpu_u32(payload, 20).unwrap()) <= 0.0
                || !matches!(gpu_u16(payload, 24), Some(1..=4))
                || gpu_u16(payload, 26) != Some(0)
            {
                return Err(anyhow!("invalid GPU surface-change event"));
            }
            Ok(())
        }
        _ => Err(anyhow!("invalid GPU event payload")),
    }
}

fn validate_gpu_text(
    payload: &[u8],
    length_offset: usize,
    flags_offset: usize,
    text_offset: usize,
) -> Result<()> {
    let Some(text_bytes) = gpu_u32(payload, length_offset).map(|value| value as usize) else {
        return Err(anyhow!("truncated GPU text event"));
    };
    let flags = if flags_offset == 6 {
        gpu_u16(payload, flags_offset).map(u32::from)
    } else {
        gpu_u32(payload, flags_offset)
    };
    let padded = text_bytes
        .checked_add(3)
        .map(|value| value & !3)
        .and_then(|value| text_offset.checked_add(value))
        .ok_or_else(|| anyhow!("GPU event text length overflow"))?;
    if text_bytes > gpu_wire::MAX_GPU_DIAGNOSTIC_BYTES
        || flags.is_none_or(|value| value > 1)
        || payload.len() != padded
        || core::str::from_utf8(&payload[text_offset..text_offset + text_bytes]).is_err()
        || payload[text_offset + text_bytes..]
            .iter()
            .any(|byte| *byte != 0)
    {
        return Err(anyhow!("invalid GPU text event"));
    }
    Ok(())
}

fn gpu_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn gpu_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn gpu_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn read_guest_memory(
    instance: &mut polkavm::RawInstance,
    pointer: u32,
    length: usize,
) -> Result<Vec<u8>> {
    if length > MAX_GUEST_READ {
        return Err(anyhow!("guest read exceeds {MAX_GUEST_READ} bytes"));
    }
    let mut bytes = vec![0; length];
    instance
        .read_memory_into(pointer, &mut bytes[..])
        .map_err(|error| anyhow!("read guest memory: {error:?}"))?;
    Ok(bytes)
}

fn map_call_result(result: Result<(), CallError<anyhow::Error>>, phase: &str) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(CallError::Trap) => Err(anyhow!("guest trapped during {phase}")),
        Err(CallError::NotEnoughGas) => Err(anyhow!("guest exhausted gas during {phase}")),
        Err(CallError::Error(error)) => Err(error).context(format!("guest error during {phase}")),
        Err(CallError::User(error)) => Err(error).context(format!("host error during {phase}")),
        Err(error) => Err(anyhow!(
            "unexpected PolkaVM error during {phase}: {error:?}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_profile_uses_tri2d_without_legacy_alias() {
        assert_eq!(
            PresentationProfile::parse("tri2d").expect("tri2d profile should parse"),
            PresentationProfile::Tri2d
        );
        assert!(
            PresentationProfile::parse("ui-mesh").is_err(),
            "the provisional profile name must not remain an alias"
        );
    }

    #[test]
    fn input_queue_coalesces_pointer_motion_and_stays_bounded() {
        let mut state = HostState::new(HashMap::new(), PresentationProfile::Framebuffer, false);
        for coordinate in 0..10_000u16 {
            state.queue_input(InputEvent {
                event_type: InputEventType::PointerMove,
                code: 0,
                x: coordinate,
                y: coordinate,
            });
        }
        assert_eq!(state.input.len(), 1);
        assert_eq!(state.input.back().unwrap().x, 9_999);

        state.queue_input(InputEvent {
            event_type: InputEventType::SurfaceMetrics,
            code: 32,
            x: 1_280,
            y: 800,
        });
        state.queue_input(InputEvent {
            event_type: InputEventType::SurfaceMetrics,
            code: 64,
            x: 2_560,
            y: 1_600,
        });
        assert_eq!(
            state
                .input
                .iter()
                .filter(|event| event.event_type == InputEventType::SurfaceMetrics)
                .count(),
            1
        );
        assert_eq!(state.input.back().unwrap().x, 2_560);

        for index in 0..(MAX_QUEUED_INPUT_EVENTS + 100) {
            state.queue_input(InputEvent {
                event_type: InputEventType::KeyDown,
                code: index as u8,
                x: 0,
                y: 0,
            });
        }
        assert_eq!(state.input.len(), MAX_QUEUED_INPUT_EVENTS);
        assert_eq!(
            state
                .input
                .iter()
                .filter(|event| event.event_type == InputEventType::SurfaceMetrics)
                .count(),
            1
        );
        state.queue_input(InputEvent {
            event_type: InputEventType::SurfaceMetrics,
            code: 64,
            x: 3_000,
            y: 2_000,
        });
        assert_eq!(state.input.len(), MAX_QUEUED_INPUT_EVENTS);
        assert_eq!(
            state.input.back().unwrap().event_type,
            InputEventType::SurfaceMetrics
        );
    }

    #[test]
    fn surface_change_rejects_non_positive_and_non_finite_scale() {
        let mut event = vec![0; gpu_wire::GPU_EVENT_HEADER_BYTES + 28];
        event[..4].copy_from_slice(&gpu_wire::GPU_EVENT_MAGIC);
        event[4..6].copy_from_slice(&gpu_wire::GPU_WIRE_VERSION.to_le_bytes());
        event[6..8].copy_from_slice(&(gpu_wire::GpuEventType::SurfaceChanged as u16).to_le_bytes());
        let event_len = event.len() as u32;
        event[8..12].copy_from_slice(&event_len.to_le_bytes());
        let payload = &mut event[gpu_wire::GPU_EVENT_HEADER_BYTES..];
        for offset in [0, 4, 8, 12, 16] {
            payload[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
        }
        payload[24..26]
            .copy_from_slice(&(gpu_wire::GpuTextureFormat::Rgba8Unorm as u16).to_le_bytes());

        payload[20..24].copy_from_slice(&1.0f32.to_le_bytes());
        validate_gpu_event(&event).expect("positive finite scale should be valid");
        for scale in [0.0f32, -1.0, f32::NAN, f32::INFINITY] {
            event[gpu_wire::GPU_EVENT_HEADER_BYTES + 20..gpu_wire::GPU_EVENT_HEADER_BYTES + 24]
                .copy_from_slice(&scale.to_le_bytes());
            assert!(
                validate_gpu_event(&event).is_err(),
                "invalid scale {scale:?} must fail closed"
            );
        }
    }
}
