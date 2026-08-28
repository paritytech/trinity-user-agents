/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![no_std]

use core::fmt;

pub const GPU_WIRE_MAGIC: [u8; 4] = *b"EPG1";
pub const GPU_WIRE_VERSION: u16 = 1;
pub const GPU_BATCH_HEADER_BYTES: usize = 24;
pub const GPU_COMMAND_HEADER_BYTES: usize = 8;
pub const MAX_GPU_BATCH_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_GPU_COMMANDS: u32 = 16_384;
pub const GPU_CAPABILITIES_MAGIC: [u8; 4] = *b"EGC1";
pub const GPU_CAPABILITIES_HEADER_BYTES: usize = 56;
pub const GPU_CAPABILITY_ENTRY_BYTES: usize = 16;
pub const GPU_EVENT_MAGIC: [u8; 4] = *b"EGE1";
pub const GPU_EVENT_HEADER_BYTES: usize = 24;
pub const MAX_GPU_EVENT_BYTES: usize = 64 * 1024;
pub const MAX_GPU_DIAGNOSTIC_BYTES: usize = 8 * 1024;
pub const GPU_HANDLE_SLOT_BITS: u32 = 20;
pub const GPU_HANDLE_SLOT_MASK: u32 = (1 << GPU_HANDLE_SLOT_BITS) - 1;
pub const GPU_HANDLE_MAX_GENERATION: u32 = (1 << (32 - GPU_HANDLE_SLOT_BITS)) - 1;
pub const MAX_GPU_SUBMITS_PER_TICK: u32 = 8;
pub const MAX_GPU_UPLOAD_BYTES_PER_TICK: usize = 16 * 1024 * 1024;
pub const MAX_GPU_QUEUED_BATCHES: usize = 4;
pub const MAX_GPU_QUEUED_EVENTS: usize = 256;
pub const MAX_GPU_BUFFERS: usize = 4_096;
pub const MAX_GPU_BUFFER_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_GPU_TOTAL_BUFFER_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_GPU_TEXTURES: usize = 512;
pub const MAX_GPU_TOTAL_TEXTURE_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_GPU_TEXTURE_DIMENSION_2D: u32 = 4_096;
pub const MAX_GPU_TEXTURE_SAMPLE_COUNT: u32 = 1;
pub const MAX_GPU_TEXTURE_MIP_LEVELS: u32 = 13;
pub const MAX_GPU_TEXTURE_VIEWS: usize = 1_024;
pub const MAX_GPU_SAMPLERS: usize = 128;
pub const MAX_GPU_SHADER_MODULES: usize = 128;
pub const MAX_GPU_WGSL_BYTES: usize = 1024 * 1024;
pub const MAX_GPU_COMPILATIONS: usize = 8;
pub const MAX_GPU_BIND_GROUP_LAYOUTS: usize = 128;
pub const MAX_GPU_PIPELINE_LAYOUTS: usize = 64;
pub const MAX_GPU_BIND_GROUPS: usize = 512;
pub const MAX_GPU_RENDER_PIPELINES: usize = 256;
pub const MAX_GPU_BIND_GROUPS_PER_PIPELINE: usize = 4;
pub const MAX_GPU_BINDINGS_PER_GROUP: usize = 16;
pub const MAX_GPU_VERTEX_BUFFERS: usize = 8;
pub const MAX_GPU_VERTEX_ATTRIBUTES: usize = 16;
pub const MAX_GPU_COLOR_ATTACHMENTS: usize = 4;
pub const MAX_GPU_RENDER_PASSES_PER_BATCH: usize = 16;
pub const MAX_GPU_DRAWS_PER_BATCH: usize = 8_192;

pub const GPU_SUBMIT_ACCEPTED: i32 = 0;
pub const GPU_SUBMIT_BUSY: i32 = 1;
pub const GPU_ERROR_INVALID_GUEST_RANGE: i32 = -1;
pub const GPU_ERROR_MALFORMED_BATCH: i32 = -2;
pub const GPU_ERROR_QUOTA_EXCEEDED: i32 = -3;
pub const GPU_ERROR_INVALID_HANDLE: i32 = -4;
pub const GPU_ERROR_INVALID_STATE: i32 = -5;
pub const GPU_ERROR_STOPPED: i32 = -6;

pub const GPU_BATCH_ERROR_STALE_SURFACE: u32 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum GpuCapabilityKey {
    MaxTextureDimension2d = 1,
    MaxBufferSize = 2,
    MaxBindingsPerBindGroup = 3,
    MaxBindGroups = 4,
    MaxVertexBuffers = 5,
    MaxVertexAttributes = 6,
    MaxColorAttachments = 7,
    MaxTextureBytes = 8,
    MaxBufferBytes = 9,
    MaxDrawsPerBatch = 10,
    MaxBatchBytes = 11,
    MaxUploadBytesPerTick = 12,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum GpuEventType {
    BatchRejected = 1,
    ShaderDiagnostic = 2,
    ResourceFailed = 3,
    UncapturedError = 4,
    SubmissionComplete = 5,
    SurfaceChanged = 6,
    DeviceLost = 7,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum GpuOpcode {
    CreateBuffer = 1,
    WriteBuffer = 2,
    CreateTexture = 3,
    WriteTexture = 4,
    CreateSampler = 5,
    CreateShaderWgsl = 6,
    CreateBindGroupLayout = 7,
    CreatePipelineLayout = 8,
    CreateBindGroup = 9,
    CreateRenderPipeline = 10,
    DestroyResource = 11,
    BeginRenderPass = 12,
    SetPipeline = 13,
    SetVertexBuffer = 14,
    SetIndexBuffer = 15,
    SetBindGroup = 16,
    SetViewport = 17,
    SetScissorRect = 18,
    Draw = 19,
    DrawIndexed = 20,
    EndRenderPass = 21,
    CopyBufferToBuffer = 22,
    CreateTextureView = 23,
}

impl TryFrom<u16> for GpuOpcode {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::CreateBuffer),
            2 => Ok(Self::WriteBuffer),
            3 => Ok(Self::CreateTexture),
            4 => Ok(Self::WriteTexture),
            5 => Ok(Self::CreateSampler),
            6 => Ok(Self::CreateShaderWgsl),
            7 => Ok(Self::CreateBindGroupLayout),
            8 => Ok(Self::CreatePipelineLayout),
            9 => Ok(Self::CreateBindGroup),
            10 => Ok(Self::CreateRenderPipeline),
            11 => Ok(Self::DestroyResource),
            12 => Ok(Self::BeginRenderPass),
            13 => Ok(Self::SetPipeline),
            14 => Ok(Self::SetVertexBuffer),
            15 => Ok(Self::SetIndexBuffer),
            16 => Ok(Self::SetBindGroup),
            17 => Ok(Self::SetViewport),
            18 => Ok(Self::SetScissorRect),
            19 => Ok(Self::Draw),
            20 => Ok(Self::DrawIndexed),
            21 => Ok(Self::EndRenderPass),
            22 => Ok(Self::CopyBufferToBuffer),
            23 => Ok(Self::CreateTextureView),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum GpuTextureFormat {
    Rgba8Unorm = 1,
    Rgba8UnormSrgb = 2,
    Bgra8Unorm = 3,
    Bgra8UnormSrgb = 4,
    Depth24Plus = 5,
    Depth32Float = 6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum GpuVertexFormat {
    Float32 = 1,
    Float32x2 = 2,
    Float32x3 = 3,
    Float32x4 = 4,
    Uint32 = 5,
    Uint32x2 = 6,
    Uint32x4 = 7,
    Unorm8x2 = 8,
    Unorm8x4 = 9,
    Snorm8x2 = 10,
    Snorm8x4 = 11,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuIndexFormat {
    Uint16 = 1,
    Uint32 = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuAddressMode {
    ClampToEdge = 1,
    Repeat = 2,
    MirrorRepeat = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuFilterMode {
    Nearest = 1,
    Linear = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuCompareFunction {
    Never = 1,
    Less = 2,
    Equal = 3,
    LessEqual = 4,
    Greater = 5,
    NotEqual = 6,
    GreaterEqual = 7,
    Always = 8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuBlendOperation {
    Add = 1,
    Subtract = 2,
    ReverseSubtract = 3,
    Min = 4,
    Max = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuBlendFactor {
    Zero = 1,
    One = 2,
    Src = 3,
    OneMinusSrc = 4,
    SrcAlpha = 5,
    OneMinusSrcAlpha = 6,
    Dst = 7,
    OneMinusDst = 8,
    DstAlpha = 9,
    OneMinusDstAlpha = 10,
    SrcAlphaSaturated = 11,
    Constant = 12,
    OneMinusConstant = 13,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuPrimitiveTopology {
    PointList = 1,
    LineList = 2,
    LineStrip = 3,
    TriangleList = 4,
    TriangleStrip = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuFrontFace {
    Ccw = 1,
    Cw = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuCullMode {
    Front = 1,
    Back = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum GpuBindingKind {
    UniformBuffer = 1,
    Sampler = 2,
    Texture = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum GpuSamplerBindingType {
    Filtering = 1,
    NonFiltering = 2,
    Comparison = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum GpuTextureSampleType {
    FloatFilterable = 1,
    FloatUnfilterable = 2,
    Depth = 3,
    Sint = 4,
    Uint = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuVertexStepMode {
    Vertex = 1,
    Instance = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuTextureDimension {
    D2 = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum GpuTextureViewDimension {
    D2 = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GpuTextureAspect {
    All = 1,
    DepthOnly = 2,
}

pub const GPU_BUFFER_USAGE_COPY_SRC: u32 = 4;
pub const GPU_BUFFER_USAGE_COPY_DST: u32 = 8;
pub const GPU_BUFFER_USAGE_INDEX: u32 = 16;
pub const GPU_BUFFER_USAGE_VERTEX: u32 = 32;
pub const GPU_BUFFER_USAGE_UNIFORM: u32 = 64;
pub const GPU_TEXTURE_USAGE_COPY_SRC: u32 = 1;
pub const GPU_TEXTURE_USAGE_COPY_DST: u32 = 2;
pub const GPU_TEXTURE_USAGE_TEXTURE_BINDING: u32 = 4;
pub const GPU_TEXTURE_USAGE_RENDER_ATTACHMENT: u32 = 16;
pub const GPU_SHADER_STAGE_VERTEX: u32 = 1;
pub const GPU_SHADER_STAGE_FRAGMENT: u32 = 2;
pub const GPU_COLOR_WRITE_RED: u16 = 1;
pub const GPU_COLOR_WRITE_GREEN: u16 = 2;
pub const GPU_COLOR_WRITE_BLUE: u16 = 4;
pub const GPU_COLOR_WRITE_ALPHA: u16 = 8;
pub const GPU_RENDER_PASS_COLOR_LOAD: u32 = 1;
pub const GPU_RENDER_PASS_COLOR_STORE: u32 = 2;
pub const GPU_RENDER_PASS_DEPTH_LOAD: u32 = 4;
pub const GPU_RENDER_PASS_DEPTH_STORE: u32 = 8;
pub const GPU_BINDING_HAS_DYNAMIC_OFFSET: u16 = 1;
pub const GPU_PIPELINE_DEPTH_WRITE: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuWireError {
    BatchTooLarge {
        actual: usize,
    },
    TruncatedBatchHeader {
        actual: usize,
    },
    InvalidMagic,
    UnsupportedVersion {
        version: u16,
    },
    ReservedBatchFlags {
        flags: u16,
    },
    BatchLengthMismatch {
        declared: usize,
        actual: usize,
    },
    EmptySequence,
    TooManyCommands {
        count: u32,
    },
    TruncatedCommandHeader {
        index: u32,
    },
    InvalidCommandLength {
        index: u32,
        length: usize,
    },
    ReservedCommandFlags {
        index: u32,
        flags: u16,
    },
    UnknownOpcode {
        index: u32,
        opcode: u16,
    },
    InvalidPayloadLength {
        index: u32,
        opcode: GpuOpcode,
        expected: usize,
        actual: usize,
    },
    InvalidWgslUtf8 {
        index: u32,
    },
    NonZeroPadding {
        index: u32,
    },
    TrailingCommandBytes {
        actual: usize,
    },
    IntegerOverflow {
        index: u32,
    },
}

impl fmt::Display for GpuWireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::BatchTooLarge { actual } => {
                write!(formatter, "GPU batch is {actual} bytes; maximum is {MAX_GPU_BATCH_BYTES}")
            }
            Self::TruncatedBatchHeader { actual } => write!(
                formatter,
                "GPU batch header is truncated: got {actual} bytes, need {GPU_BATCH_HEADER_BYTES}"
            ),
            Self::InvalidMagic => formatter.write_str("GPU batch has invalid magic"),
            Self::UnsupportedVersion { version } => {
                write!(formatter, "unsupported GPU wire version {version}")
            }
            Self::ReservedBatchFlags { flags } => {
                write!(formatter, "GPU batch has reserved flags 0x{flags:04x}")
            }
            Self::BatchLengthMismatch { declared, actual } => write!(
                formatter,
                "GPU batch declares {declared} bytes but contains {actual}"
            ),
            Self::EmptySequence => formatter.write_str("GPU batch sequence must be nonzero"),
            Self::TooManyCommands { count } => write!(
                formatter,
                "GPU batch declares {count} commands; maximum is {MAX_GPU_COMMANDS}"
            ),
            Self::TruncatedCommandHeader { index } => {
                write!(formatter, "GPU command {index} header is truncated")
            }
            Self::InvalidCommandLength { index, length } => write!(
                formatter,
                "GPU command {index} has invalid aligned length {length}"
            ),
            Self::ReservedCommandFlags { index, flags } => write!(
                formatter,
                "GPU command {index} has reserved flags 0x{flags:04x}"
            ),
            Self::UnknownOpcode { index, opcode } => {
                write!(formatter, "GPU command {index} uses unknown opcode {opcode}")
            }
            Self::InvalidPayloadLength {
                index,
                opcode,
                expected,
                actual,
            } => write!(
                formatter,
                "GPU command {index} ({opcode:?}) needs {expected} payload bytes but contains {actual}"
            ),
            Self::InvalidWgslUtf8 { index } => {
                write!(formatter, "GPU command {index} contains non-UTF-8 WGSL")
            }
            Self::NonZeroPadding { index } => {
                write!(formatter, "GPU command {index} has nonzero padding")
            }
            Self::TrailingCommandBytes { actual } => {
                write!(formatter, "GPU batch has {actual} trailing command bytes")
            }
            Self::IntegerOverflow { index } => {
                write!(formatter, "GPU command {index} payload length overflows")
            }
        }
    }
}

impl core::error::Error for GpuWireError {}

#[derive(Clone, Copy, Debug)]
pub struct GpuBatch<'a> {
    sequence: u64,
    command_count: u32,
    command_bytes: &'a [u8],
}

impl<'a> GpuBatch<'a> {
    pub fn sequence(self) -> u64 {
        self.sequence
    }

    pub fn command_count(self) -> u32 {
        self.command_count
    }

    pub fn commands(self) -> GpuCommands<'a> {
        GpuCommands {
            bytes: self.command_bytes,
            remaining: self.command_count,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GpuCommand<'a> {
    pub opcode: GpuOpcode,
    pub payload: &'a [u8],
}

pub struct GpuCommands<'a> {
    bytes: &'a [u8],
    remaining: u32,
}

impl<'a> Iterator for GpuCommands<'a> {
    type Item = GpuCommand<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let opcode = GpuOpcode::try_from(u16_at(self.bytes, 0)).ok()?;
        let command_bytes = u32_at(self.bytes, 4) as usize;
        let payload = &self.bytes[GPU_COMMAND_HEADER_BYTES..command_bytes];
        self.bytes = &self.bytes[command_bytes..];
        self.remaining -= 1;
        Some(GpuCommand { opcode, payload })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for GpuCommands<'_> {}

pub fn decode_gpu_batch(bytes: &[u8]) -> Result<GpuBatch<'_>, GpuWireError> {
    if bytes.len() > MAX_GPU_BATCH_BYTES {
        return Err(GpuWireError::BatchTooLarge {
            actual: bytes.len(),
        });
    }
    if bytes.len() < GPU_BATCH_HEADER_BYTES {
        return Err(GpuWireError::TruncatedBatchHeader {
            actual: bytes.len(),
        });
    }
    if bytes[..4] != GPU_WIRE_MAGIC {
        return Err(GpuWireError::InvalidMagic);
    }
    let version = u16_at(bytes, 4);
    if version != GPU_WIRE_VERSION {
        return Err(GpuWireError::UnsupportedVersion { version });
    }
    let flags = u16_at(bytes, 6);
    if flags != 0 {
        return Err(GpuWireError::ReservedBatchFlags { flags });
    }
    let declared_length = u32_at(bytes, 8) as usize;
    if declared_length != bytes.len() {
        return Err(GpuWireError::BatchLengthMismatch {
            declared: declared_length,
            actual: bytes.len(),
        });
    }
    let command_count = u32_at(bytes, 12);
    if command_count > MAX_GPU_COMMANDS {
        return Err(GpuWireError::TooManyCommands {
            count: command_count,
        });
    }
    let sequence = u64_at(bytes, 16);
    if sequence == 0 {
        return Err(GpuWireError::EmptySequence);
    }

    let mut command_bytes = &bytes[GPU_BATCH_HEADER_BYTES..];
    for index in 0..command_count {
        if command_bytes.len() < GPU_COMMAND_HEADER_BYTES {
            return Err(GpuWireError::TruncatedCommandHeader { index });
        }
        let opcode_value = u16_at(command_bytes, 0);
        let command_flags = u16_at(command_bytes, 2);
        if command_flags != 0 {
            return Err(GpuWireError::ReservedCommandFlags {
                index,
                flags: command_flags,
            });
        }
        let command_length = u32_at(command_bytes, 4) as usize;
        if command_length < GPU_COMMAND_HEADER_BYTES
            || !command_length.is_multiple_of(4)
            || command_length > command_bytes.len()
        {
            return Err(GpuWireError::InvalidCommandLength {
                index,
                length: command_length,
            });
        }
        let opcode =
            GpuOpcode::try_from(opcode_value).map_err(|()| GpuWireError::UnknownOpcode {
                index,
                opcode: opcode_value,
            })?;
        validate_payload(
            index,
            opcode,
            &command_bytes[GPU_COMMAND_HEADER_BYTES..command_length],
        )?;
        command_bytes = &command_bytes[command_length..];
    }
    if !command_bytes.is_empty() {
        return Err(GpuWireError::TrailingCommandBytes {
            actual: command_bytes.len(),
        });
    }

    Ok(GpuBatch {
        sequence,
        command_count,
        command_bytes: &bytes[GPU_BATCH_HEADER_BYTES..],
    })
}

fn validate_payload(index: u32, opcode: GpuOpcode, payload: &[u8]) -> Result<(), GpuWireError> {
    match opcode {
        GpuOpcode::CreateBuffer => exact_payload(index, opcode, payload, 16),
        GpuOpcode::WriteBuffer => inline_payload(index, opcode, payload, 24, 16, false),
        GpuOpcode::CreateTexture => exact_payload(index, opcode, payload, 24),
        GpuOpcode::WriteTexture => inline_payload(index, opcode, payload, 44, 40, false),
        GpuOpcode::CreateSampler => exact_payload(index, opcode, payload, 24),
        GpuOpcode::CreateShaderWgsl => inline_payload(index, opcode, payload, 8, 4, true),
        GpuOpcode::CreateBindGroupLayout => counted_payload(index, opcode, payload, 8, 4, 32),
        GpuOpcode::CreatePipelineLayout => counted_payload(index, opcode, payload, 8, 4, 4),
        GpuOpcode::CreateBindGroup => counted_payload(index, opcode, payload, 12, 8, 32),
        GpuOpcode::CreateRenderPipeline => pipeline_payload(index, opcode, payload),
        GpuOpcode::DestroyResource | GpuOpcode::SetPipeline => {
            exact_payload(index, opcode, payload, 4)
        }
        GpuOpcode::BeginRenderPass => exact_payload(index, opcode, payload, 36),
        GpuOpcode::CopyBufferToBuffer => exact_payload(index, opcode, payload, 32),
        GpuOpcode::SetVertexBuffer | GpuOpcode::SetIndexBuffer | GpuOpcode::SetViewport => {
            exact_payload(index, opcode, payload, 24)
        }
        GpuOpcode::SetBindGroup => counted_payload(index, opcode, payload, 12, 8, 4),
        GpuOpcode::SetScissorRect | GpuOpcode::Draw => exact_payload(index, opcode, payload, 16),
        GpuOpcode::DrawIndexed => exact_payload(index, opcode, payload, 20),
        GpuOpcode::EndRenderPass => exact_payload(index, opcode, payload, 0),
        GpuOpcode::CreateTextureView => exact_payload(index, opcode, payload, 20),
    }
}

fn exact_payload(
    index: u32,
    opcode: GpuOpcode,
    payload: &[u8],
    expected: usize,
) -> Result<(), GpuWireError> {
    if payload.len() != expected {
        return Err(GpuWireError::InvalidPayloadLength {
            index,
            opcode,
            expected,
            actual: payload.len(),
        });
    }
    Ok(())
}

fn inline_payload(
    index: u32,
    opcode: GpuOpcode,
    payload: &[u8],
    header_bytes: usize,
    length_offset: usize,
    utf8: bool,
) -> Result<(), GpuWireError> {
    if payload.len() < header_bytes {
        return Err(GpuWireError::InvalidPayloadLength {
            index,
            opcode,
            expected: header_bytes,
            actual: payload.len(),
        });
    }
    let inline_bytes = u32_at(payload, length_offset) as usize;
    let unpadded = header_bytes
        .checked_add(inline_bytes)
        .ok_or(GpuWireError::IntegerOverflow { index })?;
    let expected = align4(unpadded).ok_or(GpuWireError::IntegerOverflow { index })?;
    if payload.len() != expected {
        return Err(GpuWireError::InvalidPayloadLength {
            index,
            opcode,
            expected,
            actual: payload.len(),
        });
    }
    if utf8 && core::str::from_utf8(&payload[header_bytes..unpadded]).is_err() {
        return Err(GpuWireError::InvalidWgslUtf8 { index });
    }
    if payload[unpadded..].iter().any(|byte| *byte != 0) {
        return Err(GpuWireError::NonZeroPadding { index });
    }
    Ok(())
}

fn counted_payload(
    index: u32,
    opcode: GpuOpcode,
    payload: &[u8],
    header_bytes: usize,
    count_offset: usize,
    stride: usize,
) -> Result<(), GpuWireError> {
    if payload.len() < header_bytes {
        return Err(GpuWireError::InvalidPayloadLength {
            index,
            opcode,
            expected: header_bytes,
            actual: payload.len(),
        });
    }
    let count = u32_at(payload, count_offset) as usize;
    let expected = count
        .checked_mul(stride)
        .and_then(|entries| header_bytes.checked_add(entries))
        .ok_or(GpuWireError::IntegerOverflow { index })?;
    exact_payload(index, opcode, payload, expected)
}

fn pipeline_payload(index: u32, opcode: GpuOpcode, payload: &[u8]) -> Result<(), GpuWireError> {
    const HEADER_BYTES: usize = 40;
    if payload.len() < HEADER_BYTES {
        return Err(GpuWireError::InvalidPayloadLength {
            index,
            opcode,
            expected: HEADER_BYTES,
            actual: payload.len(),
        });
    }
    let vertex_layouts = u16_at(payload, 12) as usize;
    let vertex_attributes = u16_at(payload, 14) as usize;
    let color_targets = u16_at(payload, 16) as usize;
    let expected = vertex_layouts
        .checked_mul(16)
        .and_then(|value| {
            vertex_attributes
                .checked_mul(16)
                .and_then(|next| value.checked_add(next))
        })
        .and_then(|value| {
            color_targets
                .checked_mul(16)
                .and_then(|next| value.checked_add(next))
        })
        .and_then(|arrays| HEADER_BYTES.checked_add(arrays))
        .ok_or(GpuWireError::IntegerOverflow { index })?;
    exact_payload(index, opcode, payload, expected)
}

fn align4(value: usize) -> Option<usize> {
    value.checked_add(3).map(|value| value & !3)
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("validated field offset"),
    )
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated field offset"),
    )
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated field offset"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    use std::vec;
    use std::vec::Vec;

    const DESTROY_FIXTURE: [u8; 36] = [
        0x45, 0x50, 0x47, 0x31, 0x01, 0x00, 0x00, 0x00, 0x24, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x0c, 0x00,
        0x00, 0x00, 0x2a, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn decodes_golden_destroy_fixture_without_allocating() {
        let batch = decode_gpu_batch(&DESTROY_FIXTURE).expect("valid fixture");
        assert_eq!(batch.sequence(), 7);
        assert_eq!(batch.command_count(), 1);
        let commands = batch.commands().collect::<Vec<_>>();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].opcode, GpuOpcode::DestroyResource);
        assert_eq!(commands[0].payload, 42u32.to_le_bytes());
    }

    #[test]
    fn accepts_padded_utf8_shader_source() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&9u32.to_le_bytes());
        payload.extend_from_slice(&3u32.to_le_bytes());
        payload.extend_from_slice(b"abc");
        payload.push(0);
        let batch = single_command(GpuOpcode::CreateShaderWgsl, &payload);
        assert!(decode_gpu_batch(&batch).is_ok());
    }

    #[test]
    fn rejects_unknown_opcode() {
        let mut batch = DESTROY_FIXTURE;
        batch[24..26].copy_from_slice(&99u16.to_le_bytes());
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::UnknownOpcode {
                index: 0,
                opcode: 99,
            }
        );
    }

    #[test]
    fn rejects_declared_batch_length_mismatch() {
        let mut batch = DESTROY_FIXTURE;
        batch[8..12].copy_from_slice(&35u32.to_le_bytes());
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::BatchLengthMismatch {
                declared: 35,
                actual: 36,
            }
        );
    }

    #[test]
    fn rejects_extra_well_formed_command() {
        let mut batch = DESTROY_FIXTURE.to_vec();
        batch.extend_from_slice(&DESTROY_FIXTURE[24..]);
        let batch_length = batch.len() as u32;
        batch[8..12].copy_from_slice(&batch_length.to_le_bytes());
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::TrailingCommandBytes { actual: 12 }
        );
    }

    #[test]
    fn rejects_nonzero_inline_padding() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&9u32.to_le_bytes());
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(b"x");
        payload.extend_from_slice(&[0, 1, 0]);
        let batch = single_command(GpuOpcode::CreateShaderWgsl, &payload);
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::NonZeroPadding { index: 0 }
        );
    }

    #[test]
    fn rejects_non_utf8_shader_source() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&9u32.to_le_bytes());
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&[0xff, 0, 0, 0]);
        let batch = single_command(GpuOpcode::CreateShaderWgsl, &payload);
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::InvalidWgslUtf8 { index: 0 }
        );
    }

    #[test]
    fn rejects_variable_payload_count_mismatch() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&7u32.to_le_bytes());
        let batch = single_command(GpuOpcode::CreatePipelineLayout, &payload);
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::InvalidPayloadLength {
                index: 0,
                opcode: GpuOpcode::CreatePipelineLayout,
                expected: 16,
                actual: 12,
            }
        );
    }

    #[test]
    fn accepts_every_v1_opcode_payload_shape() {
        let payloads = [
            (GpuOpcode::CreateBuffer, 16),
            (GpuOpcode::WriteBuffer, 24),
            (GpuOpcode::CreateTexture, 24),
            (GpuOpcode::WriteTexture, 44),
            (GpuOpcode::CreateSampler, 24),
            (GpuOpcode::CreateShaderWgsl, 8),
            (GpuOpcode::CreateBindGroupLayout, 8),
            (GpuOpcode::CreatePipelineLayout, 8),
            (GpuOpcode::CreateBindGroup, 12),
            (GpuOpcode::CreateRenderPipeline, 40),
            (GpuOpcode::DestroyResource, 4),
            (GpuOpcode::BeginRenderPass, 36),
            (GpuOpcode::SetPipeline, 4),
            (GpuOpcode::SetVertexBuffer, 24),
            (GpuOpcode::SetIndexBuffer, 24),
            (GpuOpcode::SetBindGroup, 12),
            (GpuOpcode::SetViewport, 24),
            (GpuOpcode::SetScissorRect, 16),
            (GpuOpcode::Draw, 16),
            (GpuOpcode::DrawIndexed, 20),
            (GpuOpcode::EndRenderPass, 0),
            (GpuOpcode::CopyBufferToBuffer, 32),
            (GpuOpcode::CreateTextureView, 20),
        ];
        for (opcode, payload_bytes) in payloads {
            let batch = single_command(opcode, &vec![0; payload_bytes]);
            decode_gpu_batch(&batch)
                .unwrap_or_else(|error| panic!("{opcode:?} fixture failed: {error}"));
        }
    }

    #[test]
    fn rejects_unaligned_command_length() {
        let mut batch = DESTROY_FIXTURE;
        batch[28..32].copy_from_slice(&10u32.to_le_bytes());
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::InvalidCommandLength {
                index: 0,
                length: 10,
            }
        );
    }

    #[test]
    fn rejects_reserved_batch_flags() {
        let mut batch = DESTROY_FIXTURE;
        batch[6..8].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::ReservedBatchFlags { flags: 1 }
        );
    }

    #[test]
    fn rejects_zero_sequence() {
        let mut batch = DESTROY_FIXTURE;
        batch[16..24].fill(0);
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::EmptySequence
        );
    }

    #[test]
    fn rejects_trailing_partial_command() {
        let mut batch = DESTROY_FIXTURE.to_vec();
        batch.push(0);
        let batch_length = batch.len() as u32;
        batch[8..12].copy_from_slice(&batch_length.to_le_bytes());
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::TrailingCommandBytes { actual: 1 }
        );
    }

    #[test]
    fn rejects_batches_above_the_byte_ceiling_before_decoding() {
        let batch = vec![0; MAX_GPU_BATCH_BYTES + 1];
        assert_eq!(
            decode_gpu_batch(&batch).unwrap_err(),
            GpuWireError::BatchTooLarge {
                actual: MAX_GPU_BATCH_BYTES + 1,
            }
        );
    }

    fn single_command(opcode: GpuOpcode, payload: &[u8]) -> Vec<u8> {
        assert_eq!(payload.len() % 4, 0);
        let command_length = GPU_COMMAND_HEADER_BYTES + payload.len();
        let batch_length = GPU_BATCH_HEADER_BYTES + command_length;
        let mut bytes = Vec::with_capacity(batch_length);
        bytes.extend_from_slice(&GPU_WIRE_MAGIC);
        bytes.extend_from_slice(&GPU_WIRE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&(batch_length as u32).to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&(opcode as u16).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&(command_length as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }
}
