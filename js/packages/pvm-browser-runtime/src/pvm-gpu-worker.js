/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
/* global GPUBufferUsage, GPUMapMode, GPUTextureUsage */

"use strict";

const WIRE_VERSION = 1;
const BATCH_HEADER_BYTES = 24;
const COMMAND_HEADER_BYTES = 8;
const EVENT_HEADER_BYTES = 24;
const MAX_BATCH_BYTES = 4 * 1024 * 1024;
const MAX_COMMANDS = 16_384;
const MAX_DIAGNOSTIC_BYTES = 8 * 1024;
const HANDLE_SLOT_MASK = (1 << 20) - 1;
const HANDLE_LIVE_BIT = 1 << 12;
const MAX_COMPILATIONS_PER_BATCH = 8;
const MAX_PENDING_BATCHES = 4;
const BATCH_ERROR_STALE_SURFACE = 4;
const MAX_RENDER_PASSES_PER_BATCH = 16;
const MAX_DRAWS_PER_BATCH = 8_192;
const MAX_TOTAL_BUFFER_BYTES = 64 * 1024 * 1024;
const MAX_TOTAL_TEXTURE_BYTES = 256 * 1024 * 1024;
const resourceLimits = new Map([
  ["buffer", 4_096],
  ["texture", 512],
  ["textureView", 1_024],
  ["sampler", 128],
  ["shader", 128],
  ["bindGroupLayout", 128],
  ["pipelineLayout", 64],
  ["bindGroup", 512],
  ["pipeline", 256],
]);
const bindGroupResourceKinds = new Map([
  [1, "buffer"],
  [2, "sampler"],
  [3, "textureView"],
]);
const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder("utf-8", { fatal: true });

const formats = new Map([
  [1, "rgba8unorm"],
  [2, "rgba8unorm-srgb"],
  [3, "bgra8unorm"],
  [4, "bgra8unorm-srgb"],
  [5, "depth24plus"],
  [6, "depth32float"],
]);
const formatIds = new Map([...formats].map(([id, format]) => [format, id]));
const vertexFormats = new Map([
  [1, "float32"],
  [2, "float32x2"],
  [3, "float32x3"],
  [4, "float32x4"],
  [5, "uint32"],
  [6, "uint32x2"],
  [7, "uint32x4"],
  [8, "unorm8x2"],
  [9, "unorm8x4"],
  [10, "snorm8x2"],
  [11, "snorm8x4"],
]);
const indexFormats = new Map([
  [1, "uint16"],
  [2, "uint32"],
]);
const addressModes = new Map([
  [1, "clamp-to-edge"],
  [2, "repeat"],
  [3, "mirror-repeat"],
]);
const filterModes = new Map([
  [1, "nearest"],
  [2, "linear"],
]);
const compareFunctions = new Map([
  [1, "never"],
  [2, "less"],
  [3, "equal"],
  [4, "less-equal"],
  [5, "greater"],
  [6, "not-equal"],
  [7, "greater-equal"],
  [8, "always"],
]);
const blendOperations = new Map([
  [1, "add"],
  [2, "subtract"],
  [3, "reverse-subtract"],
  [4, "min"],
  [5, "max"],
]);
const blendFactors = new Map([
  [1, "zero"],
  [2, "one"],
  [3, "src"],
  [4, "one-minus-src"],
  [5, "src-alpha"],
  [6, "one-minus-src-alpha"],
  [7, "dst"],
  [8, "one-minus-dst"],
  [9, "dst-alpha"],
  [10, "one-minus-dst-alpha"],
  [11, "src-alpha-saturated"],
  [12, "constant"],
  [13, "one-minus-constant"],
]);
const topologies = new Map([
  [1, "point-list"],
  [2, "line-list"],
  [3, "line-strip"],
  [4, "triangle-list"],
  [5, "triangle-strip"],
]);
const frontFaces = new Map([
  [1, "ccw"],
  [2, "cw"],
]);
const cullModes = new Map([
  [0, "none"],
  [1, "front"],
  [2, "back"],
]);
const samplerBindingTypes = new Map([
  [1, "filtering"],
  [2, "non-filtering"],
  [3, "comparison"],
]);
const textureSampleTypes = new Map([
  [1, "float"],
  [2, "unfilterable-float"],
  [3, "depth"],
  [4, "sint"],
  [5, "uint"],
]);
const vertexStepModes = new Map([
  [1, "vertex"],
  [2, "instance"],
]);
const textureAspects = new Map([
  [1, "all"],
  [2, "depth-only"],
]);

class ProtocolError extends Error {
  constructor(message, commandIndex = 0xffffffff, errorCode = 1) {
    super(message);
    this.commandIndex = commandIndex;
    this.errorCode = errorCode;
  }
}

class Reader {
  constructor(bytes) {
    if (!(bytes instanceof Uint8Array)) {
      throw new ProtocolError("GPU payload is not bytes");
    }
    this.bytes = bytes;
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this.offset = 0;
  }

  take(length) {
    const end = this.offset + length;
    if (!Number.isSafeInteger(end) || end > this.bytes.byteLength) {
      throw new ProtocolError("GPU payload is truncated");
    }
    const value = this.bytes.subarray(this.offset, end);
    this.offset = end;
    return value;
  }

  u8() {
    return this.take(1)[0];
  }

  u16() {
    const offset = this.offset;
    this.take(2);
    return this.view.getUint16(offset, true);
  }

  u32() {
    const offset = this.offset;
    this.take(4);
    return this.view.getUint32(offset, true);
  }

  i32() {
    const offset = this.offset;
    this.take(4);
    return this.view.getInt32(offset, true);
  }

  u64() {
    const offset = this.offset;
    this.take(8);
    const value = this.view.getBigUint64(offset, true);
    if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new ProtocolError("GPU integer exceeds the safe range");
    }
    return Number(value);
  }

  f32() {
    const offset = this.offset;
    this.take(4);
    const value = this.view.getFloat32(offset, true);
    if (!Number.isFinite(value)) {
      throw new ProtocolError("GPU descriptor contains non-finite float");
    }
    return value;
  }

  text(length) {
    return textDecoder.decode(this.take(length));
  }

  zero(length) {
    if (this.take(length).some(byte => byte !== 0)) {
      throw new ProtocolError("GPU descriptor has nonzero reserved bytes");
    }
  }

  finish() {
    if (this.offset !== this.bytes.byteLength) {
      throw new ProtocolError("GPU descriptor has trailing bytes");
    }
  }
}

function mapped(table, value, label) {
  const result = table.get(value);
  if (result === undefined) {
    throw new ProtocolError(`unsupported GPU ${label} ${value}`);
  }
  return result;
}

function validHandle(id) {
  return id !== 0 && (id & HANDLE_SLOT_MASK) !== 0 && id >>> 20 !== 0;
}

function resource(catalog, id, kind, commandIndex) {
  const entry = catalog.get(id);
  if (!entry || (kind && entry.kind !== kind) || entry.failed) {
    throw new ProtocolError(
      `invalid ${kind || "resource"} handle ${id}`,
      commandIndex
    );
  }
  return entry;
}

function createResource(catalog, slots, id, kind, descriptor, commandIndex) {
  if (!validHandle(id) || catalog.has(id)) {
    throw new ProtocolError(`invalid new ${kind} handle ${id}`, commandIndex);
  }
  const slot = id & HANDLE_SLOT_MASK;
  const generation = id >>> 20;
  const prior = slots.get(slot);
  if (
    (prior === undefined && generation !== 1) ||
    (prior !== undefined &&
      ((prior & HANDLE_LIVE_BIT) !== 0 ||
        generation !== (prior & ~HANDLE_LIVE_BIT) + 1))
  ) {
    throw new ProtocolError(`stale new ${kind} handle ${id}`, commandIndex);
  }
  const entry = { kind, descriptor, value: null, failed: false };
  catalog.set(id, entry);
  slots.set(slot, generation | HANDLE_LIVE_BIT);
  return entry;
}

function textureByteLength(descriptor) {
  let width = descriptor.width;
  let height = descriptor.height;
  let total = 0;
  for (let level = 0; level < descriptor.mipLevelCount; level++) {
    const bytes = width * height * 4;
    if (
      !Number.isSafeInteger(bytes) ||
      total > MAX_TOTAL_TEXTURE_BYTES - bytes
    ) {
      return MAX_TOTAL_TEXTURE_BYTES + 1;
    }
    total += bytes;
    width = Math.max(1, Math.floor(width / 2));
    height = Math.max(1, Math.floor(height / 2));
  }
  return total;
}

function adjustResourceStats(stats, entry, delta, commandIndex = 0xffffffff) {
  const count = (stats.counts.get(entry.kind) || 0) + delta;
  const limit = resourceLimits.get(entry.kind);
  if (count < 0 || limit === undefined || count > limit) {
    throw new ProtocolError(
      `GPU ${entry.kind} resource quota exceeded`,
      commandIndex
    );
  }
  stats.counts.set(entry.kind, count);
  if (entry.kind === "buffer") {
    stats.bufferBytes += delta * entry.descriptor.size;
    if (stats.bufferBytes < 0 || stats.bufferBytes > MAX_TOTAL_BUFFER_BYTES) {
      throw new ProtocolError("GPU buffer memory quota exceeded", commandIndex);
    }
  } else if (entry.kind === "texture") {
    stats.textureBytes += delta * textureByteLength(entry.descriptor);
    if (
      stats.textureBytes < 0 ||
      stats.textureBytes > MAX_TOTAL_TEXTURE_BYTES
    ) {
      throw new ProtocolError(
        "GPU texture memory quota exceeded",
        commandIndex
      );
    }
  }
}

function resourceStats(catalog) {
  const stats = { counts: new Map(), bufferBytes: 0, textureBytes: 0 };
  for (const entry of catalog.values()) {
    adjustResourceStats(stats, entry, 1);
  }
  return stats;
}

function createBoundedResource(
  catalog,
  slots,
  stats,
  creations,
  id,
  kind,
  descriptor,
  commandIndex
) {
  const entry = createResource(
    catalog,
    slots,
    id,
    kind,
    descriptor,
    commandIndex
  );
  const creationCount = (creations.counts.get(kind) || 0) + 1;
  if (creationCount > resourceLimits.get(kind)) {
    throw new ProtocolError(
      `too many GPU ${kind} creations in one batch`,
      commandIndex
    );
  }
  creations.counts.set(kind, creationCount);
  if (kind === "buffer") {
    creations.bufferBytes += descriptor.size;
    if (creations.bufferBytes > MAX_TOTAL_BUFFER_BYTES) {
      throw new ProtocolError(
        "GPU buffer allocation budget exceeded",
        commandIndex
      );
    }
  } else if (kind === "texture") {
    creations.textureBytes += textureByteLength(descriptor);
    if (creations.textureBytes > MAX_TOTAL_TEXTURE_BYTES) {
      throw new ProtocolError(
        "GPU texture allocation budget exceeded",
        commandIndex
      );
    }
  } else if (kind === "shader" || kind === "pipeline") {
    creations.compilations++;
    if (creations.compilations > MAX_COMPILATIONS_PER_BATCH) {
      throw new ProtocolError(
        "too many GPU compilations in one batch",
        commandIndex
      );
    }
  }
  adjustResourceStats(stats, entry, 1, commandIndex);
  return entry;
}

function parseCommands(bytes) {
  if (
    bytes.byteLength < BATCH_HEADER_BYTES ||
    bytes.byteLength > MAX_BATCH_BYTES
  ) {
    throw new ProtocolError("GPU batch length is invalid");
  }
  const reader = new Reader(bytes);
  if (
    textDecoder.decode(reader.take(4)) !== "EPG1" ||
    reader.u16() !== WIRE_VERSION
  ) {
    throw new ProtocolError("GPU batch header is invalid");
  }
  reader.zero(2);
  if (reader.u32() !== bytes.byteLength) {
    throw new ProtocolError("GPU batch length does not match its header");
  }
  const commandCount = reader.u32();
  const sequence = reader.u64();
  if (!sequence || commandCount > MAX_COMMANDS) {
    throw new ProtocolError("GPU batch sequence or command count is invalid");
  }
  const commands = [];
  for (let index = 0; index < commandCount; index++) {
    const opcode = reader.u16();
    reader.zero(2);
    const commandBytes = reader.u32();
    if (commandBytes < COMMAND_HEADER_BYTES || commandBytes % 4) {
      throw new ProtocolError("GPU command length is invalid", index);
    }
    const payload = reader.take(commandBytes - COMMAND_HEADER_BYTES);
    commands.push({ opcode, payload, index });
  }
  reader.finish();
  return { sequence, commands };
}

// Parsing stays centralized so every opcode's exact wire shape is auditable.
// eslint-disable-next-line complexity
function parseCommand(command) {
  const reader = new Reader(command.payload);
  const result = { opcode: command.opcode, index: command.index };
  switch (command.opcode) {
    case 1:
      Object.assign(result, {
        id: reader.u32(),
        usage: reader.u32(),
        size: reader.u64(),
      });
      break;
    case 2: {
      result.id = reader.u32();
      reader.zero(4);
      result.offset = reader.u64();
      const length = reader.u32();
      reader.zero(4);
      result.data = reader.take(length).slice();
      reader.zero(reader.bytes.byteLength - reader.offset);
      break;
    }
    case 3:
      Object.assign(result, {
        id: reader.u32(),
        width: reader.u32(),
        height: reader.u32(),
        mipLevelCount: reader.u16(),
        sampleCount: reader.u16(),
        format: mapped(formats, reader.u16(), "texture format"),
      });
      if (reader.u8() !== 1) {
        throw new ProtocolError("unsupported texture dimension", command.index);
      }
      reader.zero(1);
      result.usage = reader.u32();
      break;
    case 4: {
      Object.assign(result, {
        id: reader.u32(),
        mipLevel: reader.u32(),
        origin: { x: reader.u32(), y: reader.u32(), z: reader.u32() },
        size: {
          width: reader.u32(),
          height: reader.u32(),
          depthOrArrayLayers: reader.u32(),
        },
        bytesPerRow: reader.u32(),
        rowsPerImage: reader.u32(),
      });
      const length = reader.u32();
      result.data = reader.take(length).slice();
      reader.zero(reader.bytes.byteLength - reader.offset);
      break;
    }
    case 5:
      Object.assign(result, {
        id: reader.u32(),
        addressModeU: mapped(addressModes, reader.u8(), "address mode"),
        addressModeV: mapped(addressModes, reader.u8(), "address mode"),
        addressModeW: mapped(addressModes, reader.u8(), "address mode"),
        magFilter: mapped(filterModes, reader.u8(), "filter mode"),
        minFilter: mapped(filterModes, reader.u8(), "filter mode"),
        mipmapFilter: mapped(filterModes, reader.u8(), "filter mode"),
      });
      {
        const compare = reader.u8();
        result.compare = compare
          ? mapped(compareFunctions, compare, "compare function")
          : undefined;
      }
      result.maxAnisotropy = reader.u8();
      result.lodMinClamp = reader.f32();
      result.lodMaxClamp = reader.f32();
      reader.zero(4);
      break;
    case 6: {
      result.id = reader.u32();
      const length = reader.u32();
      result.source = reader.text(length);
      reader.zero(reader.bytes.byteLength - reader.offset);
      break;
    }
    case 7: {
      result.id = reader.u32();
      const count = reader.u32();
      result.entries = [];
      for (let i = 0; i < count; i++) {
        const binding = reader.u32();
        const visibility = reader.u32();
        const kind = reader.u16();
        const flags = reader.u16();
        reader.zero(4);
        const minBindingSize = reader.u64();
        const parameter0 = reader.u32();
        const parameter1 = reader.u32();
        let entry;
        if (kind === 1) {
          if (parameter0 || parameter1 || flags & ~1) {
            throw new ProtocolError(
              "invalid buffer binding layout",
              command.index
            );
          }
          entry = {
            binding,
            visibility,
            buffer: {
              type: "uniform",
              hasDynamicOffset: Boolean(flags & 1),
              minBindingSize,
            },
          };
        } else if (kind === 2) {
          if (flags || minBindingSize || parameter1) {
            throw new ProtocolError(
              "invalid sampler binding layout",
              command.index
            );
          }
          entry = {
            binding,
            visibility,
            sampler: {
              type: mapped(
                samplerBindingTypes,
                parameter0,
                "sampler binding type"
              ),
            },
          };
        } else if (kind === 3) {
          if (flags || minBindingSize) {
            throw new ProtocolError(
              "invalid texture binding layout",
              command.index
            );
          }
          entry = {
            binding,
            visibility,
            texture: {
              sampleType: mapped(
                textureSampleTypes,
                parameter0,
                "texture sample type"
              ),
              viewDimension:
                parameter1 === 1
                  ? "2d"
                  : (() => {
                      throw new ProtocolError(
                        "unsupported texture view dimension",
                        command.index
                      );
                    })(),
              multisampled: false,
            },
          };
        } else {
          throw new ProtocolError(
            "unsupported bind group layout entry",
            command.index
          );
        }
        result.entries.push(entry);
      }
      break;
    }
    case 8: {
      result.id = reader.u32();
      const count = reader.u32();
      result.layouts = Array.from({ length: count }, () => reader.u32());
      break;
    }
    case 9: {
      result.id = reader.u32();
      result.layout = reader.u32();
      const count = reader.u32();
      result.entries = [];
      for (let i = 0; i < count; i++) {
        const binding = reader.u32();
        const resourceId = reader.u32();
        const kind = reader.u16();
        if (kind !== 1 && kind !== 2 && kind !== 3) {
          throw new ProtocolError(
            "unsupported bind group entry kind",
            command.index
          );
        }
        reader.zero(2);
        reader.zero(4);
        const offset = reader.u64();
        const size = reader.u64();
        result.entries.push({ binding, resourceId, kind, offset, size });
      }
      break;
    }
    case 10: {
      Object.assign(result, {
        id: reader.u32(),
        layout: reader.u32(),
        shader: reader.u32(),
      });
      const layoutCount = reader.u16();
      const attributeCount = reader.u16();
      const targetCount = reader.u16();
      result.flags = reader.u16();
      result.depthFormatId = reader.u16();
      result.sampleCount = reader.u16();
      result.topology = mapped(topologies, reader.u8(), "primitive topology");
      result.frontFace = mapped(frontFaces, reader.u8(), "front face");
      result.cullMode = mapped(cullModes, reader.u8(), "cull mode");
      const stripIndex = reader.u8();
      result.stripIndexFormat = stripIndex
        ? mapped(indexFormats, stripIndex, "strip index format")
        : undefined;
      const depthCompare = reader.u8();
      result.depthCompare = depthCompare
        ? mapped(compareFunctions, depthCompare, "depth compare")
        : undefined;
      reader.zero(11);
      result.layouts = [];
      for (let i = 0; i < layoutCount; i++) {
        result.layouts.push({
          arrayStride: reader.u64(),
          stepMode: mapped(vertexStepModes, reader.u8(), "vertex step mode"),
          firstAttribute: (reader.zero(3), reader.u16()),
          attributeCount: reader.u16(),
        });
      }
      result.attributes = [];
      for (let i = 0; i < attributeCount; i++) {
        result.attributes.push({
          format: mapped(vertexFormats, reader.u16(), "vertex format"),
          shaderLocation: reader.u16(),
          offset: reader.u64(),
        });
        reader.zero(4);
      }
      result.targets = [];
      for (let i = 0; i < targetCount; i++) {
        const format = mapped(formats, reader.u16(), "color target format");
        const writeMask = reader.u16();
        const color = {
          operation: mapped(blendOperations, reader.u8(), "blend operation"),
          srcFactor: mapped(blendFactors, reader.u8(), "blend factor"),
          dstFactor: mapped(blendFactors, reader.u8(), "blend factor"),
        };
        const alpha = {
          operation: mapped(blendOperations, reader.u8(), "blend operation"),
          srcFactor: mapped(blendFactors, reader.u8(), "blend factor"),
          dstFactor: mapped(blendFactors, reader.u8(), "blend factor"),
        };
        reader.zero(6);
        result.targets.push({ format, writeMask, blend: { color, alpha } });
      }
      break;
    }
    case 11:
    case 13:
      result.id = reader.u32();
      break;
    case 12:
      Object.assign(result, {
        colorView: reader.u32(),
        depthView: reader.u32(),
        surfaceGeneration: reader.u32(),
        flags: reader.u32(),
        clearColor: {
          r: reader.f32(),
          g: reader.f32(),
          b: reader.f32(),
          a: reader.f32(),
        },
        clearDepth: reader.f32(),
      });
      break;
    case 14:
      Object.assign(result, {
        slot: reader.u32(),
        buffer: reader.u32(),
        offset: reader.u64(),
        size: reader.u64(),
      });
      break;
    case 15:
      Object.assign(result, {
        buffer: reader.u32(),
        format: mapped(indexFormats, reader.u32(), "index format"),
        offset: reader.u64(),
        size: reader.u64(),
      });
      break;
    case 16: {
      result.bindIndex = reader.u32();
      result.bindGroup = reader.u32();
      const count = reader.u32();
      result.dynamicOffsets = Array.from({ length: count }, () => reader.u32());
      break;
    }
    case 17:
      result.values = Array.from({ length: 6 }, () => reader.f32());
      break;
    case 18:
    case 19:
      result.values = Array.from({ length: 4 }, () => reader.u32());
      break;
    case 20:
      result.values = [
        reader.u32(),
        reader.u32(),
        reader.u32(),
        reader.i32(),
        reader.u32(),
      ];
      break;
    case 21:
      break;
    case 22:
      Object.assign(result, {
        source: reader.u32(),
        destination: reader.u32(),
        sourceOffset: reader.u64(),
        destinationOffset: reader.u64(),
        size: reader.u64(),
      });
      break;
    case 23:
      Object.assign(result, {
        id: reader.u32(),
        texture: reader.u32(),
        format: mapped(formats, reader.u16(), "texture view format"),
      });
      if (reader.u8() !== 1) {
        throw new ProtocolError(
          "unsupported texture view dimension",
          command.index
        );
      }
      result.aspect = mapped(textureAspects, reader.u8(), "texture aspect");
      result.baseMipLevel = reader.u16();
      result.mipLevelCount = reader.u16();
      result.baseArrayLayer = reader.u16();
      result.arrayLayerCount = reader.u16();
      break;
    default:
      throw new ProtocolError(
        `unsupported GPU opcode ${command.opcode}`,
        command.index
      );
  }
  reader.finish();
  return result;
}

class GpuEngine {
  constructor(
    canvas,
    device,
    context,
    format,
    limits,
    dimensions,
    testReadback,
    testDeviceLoss
  ) {
    this.canvas = canvas;
    this.device = device;
    this.context = context;
    this.format = format;
    this.formatId = formatIds.get(format);
    this.limits = limits;
    this.resources = new Map();
    this.handleSlots = new Map();
    this.surfaceGeneration = 1;
    this.deviceGeneration = 1;
    this.lastSequence = 0;
    this.stopped = false;
    this.queue = Promise.resolve();
    this.pendingBatches = 0;
    this.testReadbacksRemaining = testReadback ? 8 : 0;
    this.testDeviceLossPending = testDeviceLoss;
    this.pendingResize = null;
    this.resizeScheduled = false;
    this.resize(dimensions, false);
    device.addEventListener("uncapturederror", event => {
      this.emitTextEvent(
        4,
        0,
        1,
        event.error?.message || "uncaptured WebGPU error"
      );
    });
    void device.lost.then(info => {
      if (!this.stopped) {
        this.stopped = true;
        this.emitTextEvent(7, 0, 1, info.message || "WebGPU device lost");
      }
    });
  }

  static async create(
    canvas,
    requirements,
    dimensions,
    testReadback,
    testDeviceLoss
  ) {
    if (!globalThis.navigator?.gpu) {
      throw new Error("WebGPU is unavailable");
    }
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) {
      throw new Error("WebGPU adapter is unavailable");
    }
    const ceilings = {
      maxTextureDimension2D: 4096,
      maxBufferSize: 16 * 1024 * 1024,
      maxBindingsPerBindGroup: 16,
      maxBindGroups: 4,
      maxVertexBuffers: 8,
      maxVertexAttributes: 16,
      maxColorAttachments: 4,
    };
    const requested = {};
    for (const [name, ceiling] of Object.entries(ceilings)) {
      const available = Number(adapter.limits[name]);
      if (!Number.isFinite(available) || available <= 0) {
        throw new Error(`WebGPU adapter omitted ${name}`);
      }
      requested[name] = Math.min(ceiling, available);
    }
    for (const [name, minimum] of Object.entries(
      requirements.requiredLimits || {}
    )) {
      if (
        !(name in requested) ||
        !Number.isSafeInteger(minimum) ||
        minimum <= 0 ||
        requested[name] < minimum
      ) {
        throw new Error(`WebGPU does not satisfy required limit ${name}`);
      }
    }
    if (
      Array.isArray(requirements.requiredFeatures) &&
      requirements.requiredFeatures.length
    ) {
      throw new Error(
        "optional WebGPU features are unavailable in profile version 1"
      );
    }
    const device = await adapter.requestDevice({
      requiredFeatures: [],
      requiredLimits: requested,
    });
    const context = canvas.getContext("webgpu");
    if (!context) {
      throw new Error("WebGPU canvas context is unavailable");
    }
    const format = navigator.gpu.getPreferredCanvasFormat();
    if (!formatIds.has(format)) {
      throw new Error(`unsupported WebGPU surface format ${format}`);
    }
    const limits = [
      requested.maxTextureDimension2D,
      requested.maxBufferSize,
      requested.maxBindingsPerBindGroup,
      requested.maxBindGroups,
      requested.maxVertexBuffers,
      requested.maxVertexAttributes,
      requested.maxColorAttachments,
      256 * 1024 * 1024,
      64 * 1024 * 1024,
      8192,
      MAX_BATCH_BYTES,
      16 * 1024 * 1024,
    ];
    return new GpuEngine(
      canvas,
      device,
      context,
      format,
      limits,
      dimensions,
      testReadback,
      testDeviceLoss
    );
  }

  resize(dimensions, notify = true) {
    const physicalWidth = Math.max(
      1,
      Math.min(this.limits[0], dimensions.physicalWidth >>> 0)
    );
    const physicalHeight = Math.max(
      1,
      Math.min(this.limits[0], dimensions.physicalHeight >>> 0)
    );
    const logicalWidth = Math.max(1, dimensions.logicalWidth >>> 0);
    const logicalHeight = Math.max(1, dimensions.logicalHeight >>> 0);
    const scale = Number(dimensions.scale);
    if (!Number.isFinite(scale) || scale <= 0) {
      throw new Error("invalid WebGPU surface scale");
    }
    const changed =
      this.physicalWidth !== physicalWidth ||
      this.physicalHeight !== physicalHeight ||
      this.logicalWidth !== logicalWidth ||
      this.logicalHeight !== logicalHeight ||
      this.scale !== scale;
    this.physicalWidth = physicalWidth;
    this.physicalHeight = physicalHeight;
    this.logicalWidth = logicalWidth;
    this.logicalHeight = logicalHeight;
    this.scale = scale;
    this.canvas.width = physicalWidth;
    this.canvas.height = physicalHeight;
    this.context.configure({
      device: this.device,
      format: this.format,
      alphaMode: "opaque",
      usage:
        GPUTextureUsage.RENDER_ATTACHMENT |
        (this.testReadbacksRemaining > 0 ? GPUTextureUsage.COPY_SRC : 0),
    });
    if (notify && changed) {
      this.surfaceGeneration++;
      postBytes("capabilities", this.capabilities());
      const payload = new Uint8Array(28);
      const view = new DataView(payload.buffer);
      for (const [offset, value] of [
        [0, this.surfaceGeneration],
        [4, physicalWidth],
        [8, physicalHeight],
        [12, logicalWidth],
        [16, logicalHeight],
      ]) {
        view.setUint32(offset, value, true);
      }
      view.setFloat32(20, scale, true);
      view.setUint16(24, this.formatId, true);
      postBytes("event", makeEvent(6, 0, payload));
    }
  }

  scheduleResize(dimensions) {
    this.pendingResize = dimensions;
    if (this.resizeScheduled) {
      return;
    }
    this.resizeScheduled = true;
    this.queue = this.queue
      .then(() => {
        const latest = this.pendingResize;
        this.pendingResize = null;
        this.resizeScheduled = false;
        if (!this.stopped && latest) {
          this.resize(latest);
        }
      })
      .catch(error => {
        postMessage({ type: "error", message: error.message || String(error) });
        this.stopped = true;
      });
  }

  capabilities() {
    const bytes = new Uint8Array(56 + this.limits.length * 16);
    const view = new DataView(bytes.buffer);
    bytes.set(textEncoder.encode("EGC1"), 0);
    view.setUint16(4, WIRE_VERSION, true);
    view.setUint32(8, bytes.byteLength, true);
    view.setUint16(12, this.formatId, true);
    view.setUint32(16, this.physicalWidth, true);
    view.setUint32(20, this.physicalHeight, true);
    view.setUint32(24, this.logicalWidth, true);
    view.setUint32(28, this.logicalHeight, true);
    view.setFloat32(32, this.scale, true);
    view.setUint32(36, this.surfaceGeneration, true);
    view.setUint32(40, this.deviceGeneration, true);
    view.setUint32(44, this.limits.length, true);
    this.limits.forEach((value, index) => {
      const offset = 56 + index * 16;
      view.setUint16(offset, index + 1, true);
      view.setBigUint64(offset + 4, BigInt(value), true);
    });
    return bytes;
  }

  submit(bytes) {
    if (this.pendingBatches >= MAX_PENDING_BATCHES) {
      this.emitBatchRejected(0xffffffff, 3, 0, "GPU submission queue is full");
      return;
    }
    this.pendingBatches++;
    this.queue = this.queue
      .then(() => this.execute(bytes))
      .catch(error => {
        postMessage({ type: "error", message: error.message || String(error) });
        this.stopped = true;
      })
      .finally(() => {
        this.pendingBatches--;
      });
  }

  // Validation is one atomic state transition across the complete opcode set.
  // eslint-disable-next-line complexity
  validate(batch) {
    if (batch.sequence <= this.lastSequence) {
      throw new ProtocolError("GPU sequence is not increasing");
    }
    const shadow = new Map(this.resources);
    const slots = new Map(this.handleSlots);
    let pass = false;
    const stats = resourceStats(shadow);
    const creations = {
      counts: new Map(),
      bufferBytes: 0,
      textureBytes: 0,
      compilations: 0,
    };
    let renderPasses = 0;
    let draws = 0;
    const commands = batch.commands.map(parseCommand);
    for (const command of commands) {
      const index = command.index;
      switch (command.opcode) {
        case 1:
          if (!command.size || command.size > this.limits[1]) {
            throw new ProtocolError("invalid buffer size", index);
          }
          command.resourceEntry = createBoundedResource(
            shadow,
            slots,
            stats,
            creations,
            command.id,
            "buffer",
            command,
            index
          );
          break;
        case 2:
          resource(shadow, command.id, "buffer", index);
          break;
        case 3:
          if (
            !command.width ||
            !command.height ||
            command.width > this.limits[0] ||
            command.height > this.limits[0] ||
            !command.mipLevelCount ||
            command.mipLevelCount > 13 ||
            command.sampleCount !== 1
          ) {
            throw new ProtocolError("invalid texture descriptor", index);
          }
          command.resourceEntry = createBoundedResource(
            shadow,
            slots,
            stats,
            creations,
            command.id,
            "texture",
            command,
            index
          );
          break;
        case 4:
          resource(shadow, command.id, "texture", index);
          break;
        case 5:
          command.resourceEntry = createBoundedResource(
            shadow,
            slots,
            stats,
            creations,
            command.id,
            "sampler",
            command,
            index
          );
          break;
        case 6:
          if (!command.source.length || command.source.length > 1024 * 1024) {
            throw new ProtocolError("invalid WGSL source", index);
          }
          command.resourceEntry = createBoundedResource(
            shadow,
            slots,
            stats,
            creations,
            command.id,
            "shader",
            command,
            index
          );
          break;
        case 7:
          if (command.entries.length > this.limits[2]) {
            throw new ProtocolError("too many bind group entries", index);
          }
          command.resourceEntry = createBoundedResource(
            shadow,
            slots,
            stats,
            creations,
            command.id,
            "bindGroupLayout",
            command,
            index
          );
          break;
        case 8:
          if (command.layouts.length > this.limits[3]) {
            throw new ProtocolError("too many pipeline bind groups", index);
          }
          command.layouts.forEach(id =>
            resource(shadow, id, "bindGroupLayout", index)
          );
          command.resourceEntry = createBoundedResource(
            shadow,
            slots,
            stats,
            creations,
            command.id,
            "pipelineLayout",
            command,
            index
          );
          break;
        case 9:
          if (command.entries.length > this.limits[2]) {
            throw new ProtocolError("too many bind group entries", index);
          }
          resource(shadow, command.layout, "bindGroupLayout", index);
          for (const entry of command.entries) {
            resource(
              shadow,
              entry.resourceId,
              bindGroupResourceKinds.get(entry.kind) || "",
              index
            );
          }
          command.resourceEntry = createBoundedResource(
            shadow,
            slots,
            stats,
            creations,
            command.id,
            "bindGroup",
            command,
            index
          );
          break;
        case 10:
          resource(shadow, command.layout, "pipelineLayout", index);
          resource(shadow, command.shader, "shader", index);
          if (
            command.layouts.length > this.limits[4] ||
            command.attributes.length > this.limits[5] ||
            command.targets.length > this.limits[6]
          ) {
            throw new ProtocolError(
              "pipeline exceeds negotiated limits",
              index
            );
          }
          command.resourceEntry = createBoundedResource(
            shadow,
            slots,
            stats,
            creations,
            command.id,
            "pipeline",
            command,
            index
          );
          break;
        case 11: {
          const removed = resource(shadow, command.id, null, index);
          adjustResourceStats(stats, removed, -1, index);
          shadow.delete(command.id);
          slots.set(command.id & HANDLE_SLOT_MASK, command.id >>> 20);
          break;
        }
        case 12:
          if (pass) {
            throw new ProtocolError("nested render pass", index);
          }
          if (command.surfaceGeneration !== this.surfaceGeneration) {
            throw new ProtocolError(
              `render pass surface generation ${command.surfaceGeneration} ` +
                `does not match current generation ${this.surfaceGeneration}`,
              index,
              BATCH_ERROR_STALE_SURFACE
            );
          }
          if (command.colorView !== 0) {
            throw new ProtocolError(
              "render pass uses a non-surface color view",
              index
            );
          }
          renderPasses++;
          if (renderPasses > MAX_RENDER_PASSES_PER_BATCH) {
            throw new ProtocolError("too many render passes", index);
          }
          if (command.depthView) {
            resource(shadow, command.depthView, "textureView", index);
          }
          pass = true;
          break;
        case 13:
          if (!pass) {
            throw new ProtocolError("pipeline set outside render pass", index);
          }
          resource(shadow, command.id, "pipeline", index);
          break;
        case 14:
          if (command.slot >= this.limits[4]) {
            throw new ProtocolError(
              "vertex buffer slot exceeds negotiated limits",
              index
            );
          }
          if (!pass) {
            throw new ProtocolError(
              "vertex buffer set outside render pass",
              index
            );
          }
          resource(shadow, command.buffer, "buffer", index);
          break;
        case 15:
          if (!pass) {
            throw new ProtocolError(
              "index buffer set outside render pass",
              index
            );
          }
          resource(shadow, command.buffer, "buffer", index);
          break;
        case 16:
          if (command.bindIndex >= this.limits[3]) {
            throw new ProtocolError(
              "bind group index exceeds negotiated limits",
              index
            );
          }
          if (!pass) {
            throw new ProtocolError(
              "bind group set outside render pass",
              index
            );
          }
          resource(shadow, command.bindGroup, "bindGroup", index);
          break;
        case 17:
        case 18:
          if (!pass) {
            throw new ProtocolError(
              "render command outside render pass",
              index
            );
          }
          break;
        case 19:
        case 20:
          if (!pass) {
            throw new ProtocolError(
              "render command outside render pass",
              index
            );
          }
          draws++;
          if (draws > MAX_DRAWS_PER_BATCH) {
            throw new ProtocolError("too many draw commands", index);
          }
          break;
        case 21:
          if (!pass) {
            throw new ProtocolError("render pass is not active", index);
          }
          pass = false;
          break;
        case 22:
          if (pass) {
            throw new ProtocolError("buffer copy inside render pass", index);
          }
          resource(shadow, command.source, "buffer", index);
          resource(shadow, command.destination, "buffer", index);
          break;
        case 23:
          resource(shadow, command.texture, "texture", index);
          command.resourceEntry = createBoundedResource(
            shadow,
            slots,
            stats,
            creations,
            command.id,
            "textureView",
            command,
            index
          );
          break;
      }
    }
    if (pass) {
      throw new ProtocolError("render pass was not ended");
    }
    return { commands, shadow, slots };
  }

  // Execution mirrors the validated opcode set and commits it transactionally.
  // eslint-disable-next-line complexity
  async execute(bytes) {
    if (this.stopped) {
      return;
    }
    let batch;
    let validated;
    try {
      batch = parseCommands(bytes);
    } catch (error) {
      throw new Error(`malformed GPU batch: ${error.message}`);
    }
    try {
      validated = this.validate(batch);
    } catch (error) {
      this.emitBatchRejected(
        error.commandIndex ?? 0xffffffff,
        error.errorCode ?? 1,
        batch.sequence,
        error.message
      );
      return;
    }
    this.device.pushErrorScope("validation");
    this.device.pushErrorScope("out-of-memory");
    const next = new Map(this.resources);
    let encoder = null;
    let pass = null;
    let surfaceView = null;
    let surfaceTexture = null;
    let readback = null;
    const removed = [];
    const shaders = [];
    const created = [];
    try {
      for (const command of validated.commands) {
        const entry = command.resourceEntry;
        switch (command.opcode) {
          case 1:
            entry.value = this.device.createBuffer({
              size: command.size,
              usage: command.usage,
            });
            next.set(command.id, entry);
            created.push(entry);
            break;
          case 2:
            this.device.queue.writeBuffer(
              resource(next, command.id, "buffer", command.index).value,
              command.offset,
              command.data
            );
            break;
          case 3:
            entry.value = this.device.createTexture({
              size: [command.width, command.height, 1],
              mipLevelCount: command.mipLevelCount,
              sampleCount: command.sampleCount,
              dimension: "2d",
              format: command.format,
              usage: command.usage,
            });
            next.set(command.id, entry);
            created.push(entry);
            break;
          case 4:
            this.device.queue.writeTexture(
              {
                texture: resource(next, command.id, "texture", command.index)
                  .value,
                mipLevel: command.mipLevel,
                origin: command.origin,
              },
              command.data,
              {
                offset: 0,
                bytesPerRow: command.bytesPerRow,
                rowsPerImage: command.rowsPerImage,
              },
              command.size
            );
            break;
          case 5:
            entry.value = this.device.createSampler(command);
            next.set(command.id, entry);
            created.push(entry);
            break;
          case 6:
            entry.value = this.device.createShaderModule({
              code: command.source,
            });
            next.set(command.id, entry);
            created.push(entry);
            shaders.push([entry, command.id]);
            break;
          case 7:
            entry.value = this.device.createBindGroupLayout({
              entries: command.entries,
            });
            next.set(command.id, entry);
            created.push(entry);
            break;
          case 8:
            entry.value = this.device.createPipelineLayout({
              bindGroupLayouts: command.layouts.map(
                id => resource(next, id, "bindGroupLayout", command.index).value
              ),
            });
            next.set(command.id, entry);
            created.push(entry);
            break;
          case 9:
            entry.value = this.device.createBindGroup({
              layout: resource(
                next,
                command.layout,
                "bindGroupLayout",
                command.index
              ).value,
              entries: command.entries.map(item => ({
                binding: item.binding,
                resource:
                  item.kind === 1
                    ? {
                        buffer: resource(
                          next,
                          item.resourceId,
                          "buffer",
                          command.index
                        ).value,
                        offset: item.offset,
                        size: item.size,
                      }
                    : resource(
                        next,
                        item.resourceId,
                        item.kind === 2 ? "sampler" : "textureView",
                        command.index
                      ).value,
              })),
            });
            next.set(command.id, entry);
            created.push(entry);
            break;
          case 10: {
            const buffers = command.layouts.map(layout => ({
              arrayStride: layout.arrayStride,
              stepMode: layout.stepMode,
              attributes: command.attributes.slice(
                layout.firstAttribute,
                layout.firstAttribute + layout.attributeCount
              ),
            }));
            const descriptor = {
              layout: resource(
                next,
                command.layout,
                "pipelineLayout",
                command.index
              ).value,
              vertex: {
                module: resource(next, command.shader, "shader", command.index)
                  .value,
                entryPoint: "vs_main",
                buffers,
              },
              fragment: {
                module: resource(next, command.shader, "shader", command.index)
                  .value,
                entryPoint: "fs_main",
                targets: command.targets,
              },
              primitive: {
                topology: command.topology,
                frontFace: command.frontFace,
                cullMode: command.cullMode,
                stripIndexFormat: command.stripIndexFormat,
              },
              multisample: { count: command.sampleCount },
            };
            if (command.depthFormatId) {
              descriptor.depthStencil = {
                format: mapped(formats, command.depthFormatId, "depth format"),
                depthWriteEnabled: Boolean(command.flags & 1),
                depthCompare: command.depthCompare,
              };
            }
            entry.value = this.device.createRenderPipeline(descriptor);
            next.set(command.id, entry);
            created.push(entry);
            break;
          }
          case 11: {
            const removedEntry = resource(
              next,
              command.id,
              null,
              command.index
            );
            removed.push(removedEntry);
            next.delete(command.id);
            break;
          }
          case 12: {
            encoder ||= this.device.createCommandEncoder();
            const colorAttachment = {
              view: (surfaceView ??= (surfaceTexture ??=
                this.context.getCurrentTexture()).createView()),
              loadOp: command.flags & 1 ? "load" : "clear",
              storeOp: command.flags & 2 ? "store" : "discard",
              clearValue: command.clearColor,
            };
            const descriptor = { colorAttachments: [colorAttachment] };
            if (command.depthView) {
              descriptor.depthStencilAttachment = {
                view: resource(
                  next,
                  command.depthView,
                  "textureView",
                  command.index
                ).value,
                depthLoadOp: command.flags & 4 ? "load" : "clear",
                depthStoreOp: command.flags & 8 ? "store" : "discard",
                depthClearValue: command.clearDepth,
              };
            }
            pass = encoder.beginRenderPass(descriptor);
            break;
          }
          case 13:
            pass.setPipeline(
              resource(next, command.id, "pipeline", command.index).value
            );
            break;
          case 14:
            pass.setVertexBuffer(
              command.slot,
              resource(next, command.buffer, "buffer", command.index).value,
              command.offset,
              command.size
            );
            break;
          case 15:
            pass.setIndexBuffer(
              resource(next, command.buffer, "buffer", command.index).value,
              command.format,
              command.offset,
              command.size
            );
            break;
          case 16:
            pass.setBindGroup(
              command.bindIndex,
              resource(next, command.bindGroup, "bindGroup", command.index)
                .value,
              command.dynamicOffsets
            );
            break;
          case 17:
            pass.setViewport(...command.values);
            break;
          case 18:
            pass.setScissorRect(...command.values);
            break;
          case 19:
            pass.draw(...command.values);
            break;
          case 20:
            pass.drawIndexed(...command.values);
            break;
          case 21:
            pass.end();
            pass = null;
            break;
          case 22:
            encoder ||= this.device.createCommandEncoder();
            encoder.copyBufferToBuffer(
              resource(next, command.source, "buffer", command.index).value,
              command.sourceOffset,
              resource(next, command.destination, "buffer", command.index)
                .value,
              command.destinationOffset,
              command.size
            );
            break;
          case 23:
            entry.value = resource(
              next,
              command.texture,
              "texture",
              command.index
            ).value.createView({
              format: command.format,
              dimension: "2d",
              aspect: command.aspect,
              baseMipLevel: command.baseMipLevel,
              mipLevelCount: command.mipLevelCount,
              baseArrayLayer: command.baseArrayLayer,
              arrayLayerCount: command.arrayLayerCount,
            });
            next.set(command.id, entry);
            created.push(entry);
            break;
        }
      }
      if (encoder && surfaceTexture && this.testReadbacksRemaining > 0) {
        readback = this.device.createBuffer({
          size: 3 * 256,
          usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
        });
        const sampleLocations = [
          [this.physicalWidth >> 1, this.physicalHeight >> 1],
          [
            Math.min(8, this.physicalWidth - 1),
            Math.min(8, this.physicalHeight - 1),
          ],
          [Math.max(0, this.physicalWidth - 20), this.physicalHeight >> 1],
        ];
        for (const [index, [x, y]] of sampleLocations.entries()) {
          encoder.copyTextureToBuffer(
            { texture: surfaceTexture, origin: { x, y } },
            {
              buffer: readback,
              offset: index * 256,
              bytesPerRow: 256,
              rowsPerImage: 1,
            },
            { width: 1, height: 1, depthOrArrayLayers: 1 }
          );
        }
      }
      if (encoder) {
        this.device.queue.submit([encoder.finish()]);
      }
    } catch (error) {
      created.forEach(entry => entry.value?.destroy?.());
      readback?.destroy();
      void this.device.popErrorScope();
      void this.device.popErrorScope();
      throw error;
    }
    const outOfMemory = await this.device.popErrorScope();
    const validation = await this.device.popErrorScope();
    const gpuError = outOfMemory || validation;
    if (gpuError) {
      readback?.destroy();
      created.forEach(entry => entry.value?.destroy?.());
      this.emitBatchRejected(
        0xffffffff,
        outOfMemory ? 2 : 1,
        batch.sequence,
        gpuError.message
      );
      return;
    }
    this.resources = next;
    this.handleSlots = validated.slots;
    this.lastSequence = batch.sequence;
    removed.forEach(entry => entry.value?.destroy?.());
    shaders.forEach(([entry, handle]) =>
      this.watchShader(entry, handle, batch.sequence)
    );
    await this.device.queue.onSubmittedWorkDone();
    if (readback) {
      await readback.mapAsync(GPUMapMode.READ);
      const readbackBytes = new Uint8Array(readback.getMappedRange());
      const samples = [0, 256, 512].map(offset =>
        Array.from(readbackBytes.subarray(offset, offset + 4))
      );
      readback.unmap();
      readback.destroy();
      this.testReadbacksRemaining--;
      postMessage({ type: "test-readback", samples });
    }
    postBytes("event", makeEvent(5, batch.sequence));
    postMessage({ type: "presented", sequence: batch.sequence });
    if (this.testDeviceLossPending && surfaceTexture) {
      this.testDeviceLossPending = false;
      this.device.destroy();
    }
  }

  async watchShader(entry, handle, sequence) {
    try {
      const info = await entry.value.getCompilationInfo();
      for (const message of info.messages) {
        let severity = 3;
        if (message.type === "error") {
          severity = 1;
        } else if (message.type === "warning") {
          severity = 2;
        }
        const text = diagnosticBytes(message.message || "shader diagnostic");
        const payload = new Uint8Array(32 + align4(text.bytes.byteLength));
        const view = new DataView(payload.buffer);
        view.setUint32(0, handle, true);
        view.setUint16(4, severity, true);
        view.setUint16(6, text.truncated ? 1 : 0, true);
        view.setUint32(8, message.lineNum || 0, true);
        view.setUint32(12, message.linePos || 0, true);
        view.setUint32(16, message.offset || 0, true);
        view.setUint32(20, message.length || 0, true);
        view.setUint32(24, text.bytes.byteLength, true);
        payload.set(text.bytes, 32);
        postBytes("event", makeEvent(2, sequence, payload));
        if (severity === 1) {
          entry.failed = true;
        }
      }
    } catch {}
  }

  emitBatchRejected(commandIndex, errorCode, sequence, message) {
    const diagnostic = diagnosticBytes(
      `surface_generation=${this.surfaceGeneration} ` +
        `physical=${this.physicalWidth}x${this.physicalHeight} ` +
        `logical=${this.logicalWidth}x${this.logicalHeight} ` +
        `scale=${this.scale} last_sequence=${this.lastSequence}: ${message}`
    );
    const payload = new Uint8Array(16 + align4(diagnostic.bytes.byteLength));
    const view = new DataView(payload.buffer);
    view.setUint32(0, commandIndex, true);
    view.setUint32(4, errorCode, true);
    view.setUint32(8, diagnostic.bytes.byteLength, true);
    view.setUint32(12, diagnostic.truncated ? 1 : 0, true);
    payload.set(diagnostic.bytes, 16);
    postBytes("event", makeEvent(1, sequence, payload));
  }

  emitTextEvent(type, sequence, errorCode, message) {
    const text = diagnosticBytes(message);
    const payload = new Uint8Array(12 + align4(text.bytes.byteLength));
    const view = new DataView(payload.buffer);
    view.setUint32(0, errorCode, true);
    view.setUint32(4, text.bytes.byteLength, true);
    view.setUint32(8, text.truncated ? 1 : 0, true);
    payload.set(text.bytes, 12);
    postBytes("event", makeEvent(type, sequence, payload));
  }

  reset() {
    this.queue = this.queue
      .then(() => {
        if (this.stopped) {
          return;
        }
        for (const entry of this.resources.values()) {
          entry.value?.destroy?.();
        }
        this.resources.clear();
        this.handleSlots.clear();
        this.pendingResize = null;
        this.lastSequence = 0;
      })
      .catch(error => {
        postMessage({ type: "error", message: error.message || String(error) });
        this.stopped = true;
      });
  }

  stop() {
    this.stopped = true;
    for (const entry of this.resources.values()) {
      entry.value?.destroy?.();
    }
    this.pendingResize = null;
    this.resources.clear();
    this.handleSlots.clear();
    this.device.destroy();
  }
}

function align4(value) {
  return (value + 3) & ~3;
}

function diagnosticBytes(message) {
  const encoded = textEncoder.encode(String(message));
  if (encoded.byteLength <= MAX_DIAGNOSTIC_BYTES) {
    return { bytes: encoded, truncated: false };
  }
  let end = MAX_DIAGNOSTIC_BYTES;
  while (end > 0) {
    try {
      textDecoder.decode(encoded.subarray(0, end));
      break;
    } catch {
      end--;
    }
  }
  return { bytes: encoded.slice(0, end), truncated: true };
}

function makeEvent(type, sequence, payload = new Uint8Array()) {
  const bytes = new Uint8Array(EVENT_HEADER_BYTES + payload.byteLength);
  const view = new DataView(bytes.buffer);
  bytes.set(textEncoder.encode("EGE1"), 0);
  view.setUint16(4, WIRE_VERSION, true);
  view.setUint16(6, type, true);
  view.setUint32(8, bytes.byteLength, true);
  view.setBigUint64(16, BigInt(sequence), true);
  bytes.set(payload, EVENT_HEADER_BYTES);
  return bytes;
}

function postBytes(type, bytes) {
  postMessage({ type, bytes }, [bytes.buffer]);
}

let engine = null;

onmessage = event => {
  const message = event.data;
  if (message?.type === "init" && !engine) {
    void GpuEngine.create(
      message.canvas,
      message.requirements || {},
      message.dimensions,
      message.testReadback === true,
      message.testDeviceLoss === true
    )
      .then(created => {
        engine = created;
        postBytes("capabilities", engine.capabilities());
      })
      .catch(error =>
        postMessage({ type: "error", message: error.message || String(error) })
      );
  } else if (message?.type === "batch" && engine) {
    engine.submit(new Uint8Array(message.bytes));
  } else if (message?.type === "resize" && engine) {
    engine.scheduleResize(message.dimensions);
  } else if (message?.type === "reset" && engine) {
    engine.reset();
  } else if (message?.type === "stop" && engine) {
    engine.stop();
    engine = null;
  }
};
