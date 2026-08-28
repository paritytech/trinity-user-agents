/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;

pub const TRI2D_MAGIC: &[u8; 4] = b"ETD1";
pub const TRI2D_VERSION: u16 = 1;
pub const TRI2D_HEADER_BYTES: usize = 24;
pub const MAX_TRI2D_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_TRI2D_COMMANDS: u32 = 8_192;
pub const MAX_TRI2D_SURFACE_SIZE: u32 = 4_096;
pub const MAX_TRI2D_TEXTURE_SIZE: u32 = 4_096;
pub const MAX_TRI2D_TEXTURES: usize = 256;
pub const MAX_TRI2D_TEXTURE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TRI2D_DRAWS: u32 = 4_096;
pub const MAX_TRI2D_VERTICES: u32 = 262_144;
pub const MAX_TRI2D_INDICES: u32 = 786_432;

const VERTEX_BYTES: usize = 20;
const OPCODE_TEXTURE_CREATE: u8 = 1;
const OPCODE_TEXTURE_UPDATE: u8 = 2;
const OPCODE_TEXTURE_DESTROY: u8 = 3;
const OPCODE_DRAW: u8 = 4;
const OPCODE_PRESENT: u8 = 5;

#[derive(Clone, Copy, Debug)]
struct Texture {
    width: u32,
    height: u32,
    bytes: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Tri2dState {
    textures: HashMap<u32, Texture>,
    texture_bytes: usize,
}

#[derive(Debug)]
pub struct Tri2dFrame {
    pub width: u32,
    pub height: u32,
    pub draw_count: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Tri2dMetadata {
    pub width: u32,
    pub height: u32,
    pub draw_count: u32,
    pub vertex_count: u32,
    pub index_count: u32,
}

pub(crate) fn validate_tri2d(
    bytes: &[u8],
    current: &Tri2dState,
) -> Result<(Tri2dState, Tri2dMetadata)> {
    if bytes.len() < TRI2D_HEADER_BYTES || bytes.len() > MAX_TRI2D_BYTES {
        bail!("tri2d stream has invalid byte length");
    }
    if &bytes[..4] != TRI2D_MAGIC {
        bail!("tri2d stream has invalid magic");
    }

    let mut reader = Reader::new(bytes);
    reader.skip(4)?;
    if reader.u16()? != TRI2D_VERSION {
        bail!("tri2d stream has unsupported version");
    }
    if reader.u16()? as usize != TRI2D_HEADER_BYTES {
        bail!("tri2d stream has invalid header length");
    }
    let width = reader.u32()?;
    let height = reader.u32()?;
    if width == 0
        || height == 0
        || width > MAX_TRI2D_SURFACE_SIZE
        || height > MAX_TRI2D_SURFACE_SIZE
    {
        bail!("tri2d stream has invalid surface dimensions");
    }
    let command_count = reader.u32()?;
    if command_count == 0 || command_count > MAX_TRI2D_COMMANDS {
        bail!("tri2d stream has invalid command count");
    }
    let _clear_rgba = reader.u32()?;

    let mut state = current.clone();
    let mut draw_count = 0u32;
    let mut vertex_count = 0u32;
    let mut index_count = 0u32;
    let mut presented = false;

    for command_index in 0..command_count {
        let opcode = reader.u8()?;
        if reader.u8()? != 0 || reader.u16()? != 0 {
            bail!("tri2d command has unsupported flags");
        }
        let payload_length = reader.u32()? as usize;
        let payload = reader.bytes(payload_length)?;
        let mut payload = Reader::new(payload);

        if presented {
            bail!("tri2d command follows present");
        }
        match opcode {
            OPCODE_TEXTURE_CREATE => {
                let handle = nonzero_handle(payload.u32()?)?;
                let texture_width = payload.u32()?;
                let texture_height = payload.u32()?;
                let filter = payload.u32()?;
                let byte_length = payload.u32()? as usize;
                if texture_width == 0
                    || texture_height == 0
                    || texture_width > MAX_TRI2D_TEXTURE_SIZE
                    || texture_height > MAX_TRI2D_TEXTURE_SIZE
                    || filter > 1
                {
                    bail!("tri2d texture create has invalid properties");
                }
                let expected = pixel_bytes(texture_width, texture_height)?;
                if byte_length != expected || payload.remaining() != byte_length {
                    bail!("tri2d texture create has invalid pixel length");
                }
                payload.skip(byte_length)?;
                if state.textures.contains_key(&handle) {
                    bail!("tri2d texture handle already exists");
                }
                if state.textures.len() == MAX_TRI2D_TEXTURES
                    || state.texture_bytes.saturating_add(byte_length) > MAX_TRI2D_TEXTURE_BYTES
                {
                    bail!("tri2d texture limits exceeded");
                }
                state.textures.insert(
                    handle,
                    Texture {
                        width: texture_width,
                        height: texture_height,
                        bytes: byte_length,
                    },
                );
                state.texture_bytes += byte_length;
            }
            OPCODE_TEXTURE_UPDATE => {
                let handle = nonzero_handle(payload.u32()?)?;
                let x = payload.u32()?;
                let y = payload.u32()?;
                let update_width = payload.u32()?;
                let update_height = payload.u32()?;
                let byte_length = payload.u32()? as usize;
                let texture = state
                    .textures
                    .get(&handle)
                    .ok_or_else(|| anyhow!("tri2d texture update uses an unknown handle"))?;
                if update_width == 0
                    || update_height == 0
                    || x.checked_add(update_width)
                        .is_none_or(|right| right > texture.width)
                    || y.checked_add(update_height)
                        .is_none_or(|bottom| bottom > texture.height)
                {
                    bail!("tri2d texture update exceeds texture bounds");
                }
                let expected = pixel_bytes(update_width, update_height)?;
                if byte_length != expected || payload.remaining() != byte_length {
                    bail!("tri2d texture update has invalid pixel length");
                }
                payload.skip(byte_length)?;
            }
            OPCODE_TEXTURE_DESTROY => {
                let handle = nonzero_handle(payload.u32()?)?;
                let texture = state
                    .textures
                    .remove(&handle)
                    .ok_or_else(|| anyhow!("tri2d texture destroy uses an unknown handle"))?;
                state.texture_bytes -= texture.bytes;
            }
            OPCODE_DRAW => {
                let handle = nonzero_handle(payload.u32()?)?;
                if !state.textures.contains_key(&handle) {
                    bail!("tri2d draw uses an unknown texture handle");
                }
                let clip_x = payload.u32()?;
                let clip_y = payload.u32()?;
                let clip_width = payload.u32()?;
                let clip_height = payload.u32()?;
                if clip_width == 0
                    || clip_height == 0
                    || clip_x
                        .checked_add(clip_width)
                        .is_none_or(|right| right > width)
                    || clip_y
                        .checked_add(clip_height)
                        .is_none_or(|bottom| bottom > height)
                {
                    bail!("tri2d draw has an invalid clip rectangle");
                }
                let vertices = payload.u32()?;
                let indices = payload.u32()?;
                if vertices == 0 || indices == 0 || indices % 3 != 0 {
                    bail!("tri2d draw has invalid element counts");
                }
                draw_count = draw_count
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("tri2d draw count overflow"))?;
                vertex_count = vertex_count
                    .checked_add(vertices)
                    .ok_or_else(|| anyhow!("tri2d vertex count overflow"))?;
                index_count = index_count
                    .checked_add(indices)
                    .ok_or_else(|| anyhow!("tri2d index count overflow"))?;
                if draw_count > MAX_TRI2D_DRAWS
                    || vertex_count > MAX_TRI2D_VERTICES
                    || index_count > MAX_TRI2D_INDICES
                {
                    bail!("tri2d draw limits exceeded");
                }
                let vertex_bytes = (vertices as usize)
                    .checked_mul(VERTEX_BYTES)
                    .ok_or_else(|| anyhow!("tri2d vertex byte length overflow"))?;
                let index_bytes = (indices as usize)
                    .checked_mul(4)
                    .ok_or_else(|| anyhow!("tri2d index byte length overflow"))?;
                if payload.remaining() != vertex_bytes.saturating_add(index_bytes) {
                    bail!("tri2d draw has invalid payload length");
                }
                payload.skip(vertex_bytes)?;
                for _ in 0..indices {
                    if payload.u32()? >= vertices {
                        bail!("tri2d draw index exceeds vertex count");
                    }
                }
            }
            OPCODE_PRESENT => {
                if payload_length != 0 || command_index + 1 != command_count {
                    bail!("tri2d present must be the final empty command");
                }
                presented = true;
            }
            _ => bail!("tri2d stream has unknown opcode {opcode}"),
        }
        if payload.remaining() != 0 {
            bail!("tri2d command has trailing payload bytes");
        }
    }

    if !presented || reader.remaining() != 0 {
        bail!("tri2d stream has invalid presentation boundary");
    }

    Ok((
        state,
        Tri2dMetadata {
            width,
            height,
            draw_count,
            vertex_count,
            index_count,
        },
    ))
}

fn nonzero_handle(handle: u32) -> Result<u32> {
    if handle == 0 {
        bail!("tri2d texture handle must be nonzero");
    }
    Ok(handle)
}

fn pixel_bytes(width: u32, height: u32) -> Result<usize> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| anyhow!("tri2d texture byte length overflow"))
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| anyhow!("truncated tri2d stream"))?;
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn skip(&mut self, length: usize) -> Result<()> {
        self.bytes(length).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let bytes: [u8; 2] = self
            .bytes(2)?
            .try_into()
            .expect("reader returns the requested byte length");
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self
            .bytes(4)?
            .try_into()
            .expect("reader returns the requested byte length");
        Ok(u32::from_le_bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![opcode, 0, 0, 0];
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn stream(commands: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(TRI2D_MAGIC);
        bytes.extend_from_slice(&TRI2D_VERSION.to_le_bytes());
        bytes.extend_from_slice(&(TRI2D_HEADER_BYTES as u16).to_le_bytes());
        bytes.extend_from_slice(&64u32.to_le_bytes());
        bytes.extend_from_slice(&48u32.to_le_bytes());
        bytes.extend_from_slice(&(commands.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0xff000000u32.to_le_bytes());
        for command in commands {
            bytes.extend_from_slice(command);
        }
        bytes
    }

    fn create(handle: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&handle.to_le_bytes());
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(&4u32.to_le_bytes());
        payload.extend_from_slice(&[255, 255, 255, 255]);
        command(OPCODE_TEXTURE_CREATE, &payload)
    }

    fn draw(handle: u32, last_index: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        for value in [handle, 0, 0, 64, 48, 3, 3] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        payload.extend_from_slice(&[0; VERTEX_BYTES * 3]);
        for index in [0u32, 1, last_index] {
            payload.extend_from_slice(&index.to_le_bytes());
        }
        command(OPCODE_DRAW, &payload)
    }

    fn present() -> Vec<u8> {
        command(OPCODE_PRESENT, &[])
    }

    #[test]
    fn accepts_a_stateful_texture_and_draw_sequence() {
        let state = Tri2dState::default();
        let first = stream(&[create(7), draw(7, 2), present()]);
        let (state, metadata) =
            validate_tri2d(&first, &state).expect("initial tri2d should validate");
        assert_eq!((metadata.width, metadata.height), (64, 48));
        assert_eq!(metadata.draw_count, 1);
        assert_eq!(metadata.vertex_count, 3);
        assert_eq!(metadata.index_count, 3);
        assert_eq!(state.textures.len(), 1);

        let second = stream(&[draw(7, 2), present()]);
        let (_, metadata) =
            validate_tri2d(&second, &state).expect("retained texture should be reusable");
        assert_eq!(metadata.draw_count, 1);
    }

    #[test]
    fn rejects_malformed_and_out_of_bounds_streams() {
        let state = Tri2dState::default();
        let mut invalid_magic = stream(&[present()]);
        invalid_magic[0] = b'X';
        assert!(validate_tri2d(&invalid_magic, &state).is_err());

        let truncated = &stream(&[create(1), present()])[..31];
        assert!(validate_tri2d(truncated, &state).is_err());

        let unknown_texture = stream(&[draw(9, 2), present()]);
        assert!(validate_tri2d(&unknown_texture, &state).is_err());

        let invalid_index = stream(&[create(1), draw(1, 3), present()]);
        assert!(validate_tri2d(&invalid_index, &state).is_err());

        let command_after_present = stream(&[present(), present()]);
        assert!(validate_tri2d(&command_after_present, &state).is_err());
    }

    #[test]
    fn failed_submission_does_not_mutate_retained_texture_state() {
        let state = Tri2dState::default();
        let missing_present = stream(&[create(11)]);
        assert!(validate_tri2d(&missing_present, &state).is_err());

        let valid = stream(&[create(11), present()]);
        let (state, _) =
            validate_tri2d(&valid, &state).expect("failed validation must not retain textures");
        assert!(state.textures.contains_key(&11));
    }
}
