/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::corevm::{Interruption, Vm};
use crate::{
    AudioChunk, Frame, GpuBatch, InputEvent, InputEventType, PresentationProfile, Runtime,
    Tri2dFrame, MAX_FRAME_BYTES, MAX_GUEST_RW_DATA_BYTES, MAX_GUEST_STACK_BYTES,
};
use anyhow::{anyhow, Context, Result};
use polkavm::ProgramBlob;
use std::collections::{HashMap, VecDeque};

const MAX_INTERRUPTS_PER_UPDATE: usize = 8_192;
const MAX_QUEUED_AUDIO_CHUNKS: usize = 64;

// Both variants are large, long-lived runtime state machines. Boxing either
// adds allocation and indirection to every host call to save 576 enum bytes.
#[allow(clippy::large_enum_variant)]
pub enum ApplicationRuntime {
    Cooperative(Runtime),
    CoreVm(CoreVmRuntime),
}

pub struct CoreVmRuntime {
    vm: Vm,
    frame: Option<Frame>,
    audio: VecDeque<AudioChunk>,
    palette: [[u8; 3]; 256],
    sample_rate: u32,
    channels: u32,
    pointer: Option<(u16, u16)>,
    audio_enabled: bool,
    max_gas_per_update: u64,
    exited: bool,
}

impl ApplicationRuntime {
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
            crate::preferred_backend(),
        )
    }

    pub fn new_with_backend(
        program: &[u8],
        assets: HashMap<String, Vec<u8>>,
        presentation: PresentationProfile,
        audio_enabled: bool,
        max_gas_per_update: u64,
        backend: polkavm::BackendKind,
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
        let is_corevm = blob.exports().any(|export| export.symbol() == "_pvm_start");
        if !is_corevm {
            return Runtime::new_with_backend(
                program,
                assets,
                presentation,
                audio_enabled,
                max_gas_per_update,
                backend,
            )
            .map(Self::Cooperative);
        }
        if presentation != PresentationProfile::Framebuffer {
            return Err(anyhow!(
                "CoreVM guests require the framebuffer presentation profile"
            ));
        }

        let mut vm = Vm::from_blob(blob, backend).context("create CoreVM guest")?;
        for (path, bytes) in assets {
            vm.register_file(&path, bytes);
        }
        vm.setup(["./quake"]).map_err(|error| anyhow!(error))?;
        Ok(Self::CoreVm(CoreVmRuntime {
            vm,
            frame: None,
            audio: VecDeque::new(),
            palette: [[255; 3]; 256],
            sample_rate: 0,
            channels: 0,
            audio_enabled,
            pointer: None,
            max_gas_per_update,
            exited: false,
        }))
    }

    pub fn init(&mut self) -> Result<()> {
        match self {
            Self::Cooperative(runtime) => runtime.init(),
            Self::CoreVm(_) => Ok(()),
        }
    }

    pub fn update(&mut self) -> Result<()> {
        match self {
            Self::Cooperative(runtime) => runtime.update(),
            Self::CoreVm(runtime) => runtime.update(),
        }
    }

    pub fn backend(&self) -> polkavm::BackendKind {
        match self {
            Self::Cooperative(runtime) => runtime.backend(),
            Self::CoreVm(runtime) => runtime.vm.backend(),
        }
    }
    pub fn last_gas_used(&self) -> u64 {
        match self {
            Self::Cooperative(runtime) => runtime.last_gas_used(),
            Self::CoreVm(runtime) => runtime
                .max_gas_per_update
                .saturating_sub(runtime.vm.gas_remaining()),
        }
    }

    pub fn send_input(&mut self, event: InputEvent) {
        match self {
            Self::Cooperative(runtime) => runtime.send_input(event),
            Self::CoreVm(runtime) => runtime.send_input(event),
        }
    }

    pub fn gpu_ready(&self) -> bool {
        match self {
            Self::Cooperative(runtime) => runtime.gpu_ready(),
            Self::CoreVm(_) => true,
        }
    }

    pub fn set_gpu_capabilities(&mut self, bytes: Vec<u8>) -> Result<()> {
        match self {
            Self::Cooperative(runtime) => runtime.set_gpu_capabilities(bytes),
            Self::CoreVm(_) => Err(anyhow!("CoreVM does not support GPU capabilities")),
        }
    }

    pub fn send_gpu_event(&mut self, bytes: Vec<u8>) -> Result<()> {
        match self {
            Self::Cooperative(runtime) => runtime.send_gpu_event(bytes),
            Self::CoreVm(_) => Err(anyhow!("CoreVM does not support GPU events")),
        }
    }

    pub fn take_gpu_batch(&mut self) -> Option<GpuBatch> {
        match self {
            Self::Cooperative(runtime) => runtime.take_gpu_batch(),
            Self::CoreVm(_) => None,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn set_time_ms(&mut self, time_ms: u64) {
        if let Self::Cooperative(runtime) = self {
            runtime.set_time_ms(time_ms);
        }
    }

    pub fn take_frame(&mut self) -> Option<Frame> {
        match self {
            Self::Cooperative(runtime) => runtime.take_frame(),
            Self::CoreVm(runtime) => runtime.frame.take(),
        }
    }
    pub fn take_tri2d(&mut self) -> Option<Tri2dFrame> {
        match self {
            Self::Cooperative(runtime) => runtime.take_tri2d(),
            Self::CoreVm(_) => None,
        }
    }

    pub fn take_audio(&mut self) -> Option<AudioChunk> {
        match self {
            Self::Cooperative(runtime) => runtime.take_audio(),
            Self::CoreVm(runtime) => runtime.audio.pop_front(),
        }
    }

    pub fn take_log(&mut self) -> Option<String> {
        match self {
            Self::Cooperative(runtime) => runtime.take_log(),
            Self::CoreVm(_) => None,
        }
    }

    pub fn is_exited(&self) -> bool {
        matches!(self, Self::CoreVm(runtime) if runtime.exited)
    }

    pub fn take_save(&mut self) -> Option<Vec<u8>> {
        match self {
            Self::Cooperative(runtime) => runtime.take_save(),
            Self::CoreVm(_) => None,
        }
    }
}

impl CoreVmRuntime {
    fn update(&mut self) -> Result<()> {
        if self.exited {
            return Ok(());
        }
        self.vm.set_gas(self.max_gas_per_update);
        for _ in 0..MAX_INTERRUPTS_PER_UPDATE {
            match self.vm.run().map_err(|error| anyhow!(error))? {
                Interruption::Exit => {
                    self.exited = true;
                    return Ok(());
                }
                Interruption::Yield => return Ok(()),
                Interruption::SetPalette { palette } => {
                    if palette.len() != 256 * 3 {
                        return Err(anyhow!("guest supplied an invalid Quake palette"));
                    }
                    for (target, source) in self.palette.iter_mut().zip(palette.as_chunks::<3>().0)
                    {
                        target.copy_from_slice(source);
                    }
                }
                Interruption::Display {
                    width,
                    height,
                    framebuffer,
                } => {
                    let width = u32::try_from(width).context("Quake frame width overflow")?;
                    let height = u32::try_from(height).context("Quake frame height overflow")?;
                    let pixels = width
                        .checked_mul(height)
                        .ok_or_else(|| anyhow!("Quake frame dimensions overflow"))?
                        as usize;
                    if pixels == 0 || pixels > MAX_FRAME_BYTES / 4 || framebuffer.len() != pixels {
                        return Err(anyhow!("guest supplied an invalid Quake frame"));
                    }
                    let mut argb = Vec::with_capacity(pixels * 4);
                    for index in framebuffer {
                        let [red, green, blue] = self.palette[index as usize];
                        argb.extend_from_slice(&[blue, green, red, 255]);
                    }
                    self.frame = Some(Frame {
                        width,
                        height,
                        argb,
                    });
                    return Ok(());
                }
                Interruption::AudioInit {
                    channels,
                    sample_rate,
                } => {
                    if !self.audio_enabled {
                        self.channels = 0;
                        self.sample_rate = 0;
                        continue;
                    }
                    if !(1..=2).contains(&channels) || !(8_000..=96_000).contains(&sample_rate) {
                        return Err(anyhow!("guest requested an unsupported audio format"));
                    }
                    self.channels = channels;
                    self.sample_rate = sample_rate;
                }
                Interruption::AudioFrame { buffer } => {
                    if !self.audio_enabled {
                        continue;
                    }
                    if self.channels == 0 {
                        self.channels = crate::AUDIO_CHANNELS;
                        self.sample_rate = crate::AUDIO_SAMPLE_RATE;
                    }
                    if buffer.is_empty() || buffer.len() % self.channels as usize != 0 {
                        continue;
                    }
                    if self.audio.len() == MAX_QUEUED_AUDIO_CHUNKS {
                        self.audio.pop_front();
                    }
                    self.audio.push_back(AudioChunk {
                        samples: buffer,
                        sample_rate: self.sample_rate,
                        channels: self.channels,
                    });
                }
            }
        }
        Err(anyhow!("guest exceeded interruption budget"))
    }

    fn send_input(&mut self, event: InputEvent) {
        if self.vm.uses_epoca_inputs() {
            self.vm.send_epoca_input(event);
            return;
        }
        match event.event_type {
            InputEventType::KeyDown | InputEventType::KeyUp => {
                if let Some(key) = crate::quake_keys::from_hid(event.code) {
                    self.vm
                        .send_key(key, event.event_type == InputEventType::KeyDown);
                }
            }
            InputEventType::ButtonDown | InputEventType::ButtonUp => {
                if let Some(key) = crate::quake_keys::from_button(event.code) {
                    self.vm
                        .send_key(key, event.event_type == InputEventType::ButtonDown);
                }
            }
            InputEventType::PointerMove => {
                if let Some((previous_x, previous_y)) = self.pointer {
                    self.vm.send_mouse_move(
                        signed_delta(event.x, previous_x),
                        signed_delta(event.y, previous_y),
                    );
                }
                self.pointer = Some((event.x, event.y));
            }
            InputEventType::PointerDelta => self.vm.send_mouse_move(
                (event.x as i16).clamp(i8::MIN as i16, i8::MAX as i16) as i8,
                (event.y as i16).clamp(i8::MIN as i16, i8::MAX as i16) as i8,
            ),
            InputEventType::SurfaceMetrics => {}
        }
    }
}

fn signed_delta(current: u16, previous: u16) -> i8 {
    (i32::from(current) - i32::from(previous)).clamp(i8::MIN as i32, i8::MAX as i32) as i8
}
