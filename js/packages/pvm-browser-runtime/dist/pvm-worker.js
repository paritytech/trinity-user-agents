/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

(() => {
  const STATUS_FINISHED = -1;
  const STATUS_ECALL = -2;
  const STATUS_TRAP = -3;
  const STATUS_OUT_OF_GAS = -4;
  const INPUT_EVENT_BYTES = 8;
  const MAX_INPUT_EVENTS = 4096;
  const MAX_HOSTCALLS_PER_INIT = 1024 * 1024;
  const MAX_HOSTCALLS_PER_UPDATE = 8192;
  const MAX_HOSTCALL_BYTES = 32 * 1024 * 1024;
  const MAX_LOG_BYTES = 4 * 1024;
  const MAX_SAVE_BYTES = 1024 * 1024;
  const MAX_AUDIO_SAMPLES = 48000 * 2;
  const MAX_FRAME_BYTES = 16 * 1024 * 1024;
  const MAX_TRI2D_BYTES = 8 * 1024 * 1024;
  const MAX_GPU_BATCH_BYTES = 4 * 1024 * 1024;
  const MAX_GPU_EVENT_BYTES = 64 * 1024;
  const MAX_GPU_EVENTS = 256;
  const MAX_GPU_SUBMITS_PER_UPDATE = 8;
  const MAX_GPU_COMMANDS = 16_384;
  const GPU_ERROR_MALFORMED_BATCH = -2;
  const GPU_ERROR_QUOTA_EXCEEDED = -3;
  const GPU_ERROR_INVALID_STATE = -5;
  const IOV_MAX = 1024n;
  const AT_FDCWD = BigInt.asUintN(64, -100n);
  const ENOSYS = 38;
  const EFAULT = 14;
  const ENOENT = 2;
  const EBADF = 9;
  const EACCES = 13;
  const EINVAL = 22;
  const SYS_OPENAT = 56n;
  const SYS_CLOSE = 57n;
  const SYS_LSEEK = 62n;
  const SYS_READ = 63n;
  const SYS_READV = 65n;
  const SYS_WRITEV = 66n;
  const SYS_EXIT = 93n;
  const decoder = new TextDecoder();
  const encoder = new TextEncoder();

  function readMetadata(module) {
    const sections = WebAssembly.Module.customSections(
      module,
      "epoca.pvm.meta",
    );
    if (sections.length !== 1) {
      throw new Error("translated PolkaVM module has invalid metadata");
    }
    const bytes = new Uint8Array(sections[0]);
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    let offset = 0;
    const requireBytes = (length) => {
      if (offset + length > bytes.byteLength) {
        throw new Error("translated PolkaVM metadata is truncated");
      }
    };
    const readU16 = () => {
      requireBytes(2);
      const value = view.getUint16(offset, true);
      offset += 2;
      return value;
    };
    const readU32 = () => {
      requireBytes(4);
      const value = view.getUint32(offset, true);
      offset += 4;
      return value;
    };
    const readString = (length) => {
      requireBytes(length);
      const value = decoder.decode(bytes.subarray(offset, offset + length));
      offset += length;
      return value;
    };
    requireBytes(4);
    if (decoder.decode(bytes.subarray(0, 4)) !== "EPM2") {
      throw new Error("translated PolkaVM metadata has an incompatible ABI");
    }
    offset = 4;
    const is64Bit = readU32() !== 0;
    const names = [
      "roAddress",
      "roSize",
      "roPhysical",
      "rwAddress",
      "rwSize",
      "rwPhysical",
      "heapBase",
      "heapLimit",
      "stackLow",
      "stackHigh",
      "stackPhysical",
    ];
    const layout = {};
    for (const name of names) {
      layout[name] = readU32();
    }
    const imports = [];
    const importCount = readU32();
    for (let index = 0; index < importCount; index++) {
      const length = readU16();
      imports.push(length ? readString(length) : null);
    }
    const exports = new Map();
    const exportCount = readU32();
    for (let index = 0; index < exportCount; index++) {
      const name = readString(readU16());
      const block = readU32();
      if (!name || exports.has(name)) {
        throw new Error("translated PolkaVM metadata has invalid exports");
      }
      exports.set(name, block);
    }
    if (offset !== bytes.byteLength) {
      throw new Error("translated PolkaVM metadata has trailing data");
    }
    return { is64Bit, layout, imports, exports };
  }

  function errno(code) {
    return BigInt.asUintN(64, -BigInt(code));
  }

  function normalizedPath(path) {
    while (path.startsWith("./")) {
      path = path.slice(2);
    }
    return path.replace(/^\/+/, "");
  }

  function signedByte(value) {
    return value > 127 ? value - 256 : value;
  }

  function hidToCoreVm(code) {
    if (code >= 0x04 && code <= 0x1d) {
      return 0x61 + code - 0x04;
    }
    if (code >= 0x1e && code <= 0x26) {
      return 0x31 + code - 0x1e;
    }
    const keys = new Map([
      [0x27, 0x30],
      [0x28, 0x0a],
      [0x58, 0x0a],
      [0x29, 0x1b],
      [0x2a, 0x08],
      [0x2b, 0x09],
      [0x2c, 0x20],
      [0x2d, 0x2d],
      [0x56, 0x2d],
      [0x2e, 0x3d],
      [0x2f, 0x5b],
      [0x30, 0x5d],
      [0x31, 0x5c],
      [0x33, 0x3b],
      [0x34, 0x27],
      [0x35, 0x60],
      [0x36, 0x2c],
      [0x37, 0x2e],
      [0x63, 0x2e],
      [0x38, 0x2f],
      [0x54, 0x2f],
      [0x46, 0x91],
      [0x47, 0x92],
      [0x48, 0x93],
      [0x49, 0x94],
      [0x4a, 0x96],
      [0x4b, 0x98],
      [0x4c, 0x95],
      [0x4d, 0x97],
      [0x4e, 0x99],
      [0x4f, 0x82],
      [0x50, 0x83],
      [0x51, 0x81],
      [0x52, 0x80],
      [0x55, 0x2a],
      [0x57, 0x2b],
      [0x59, 0x97],
      [0x5a, 0x81],
      [0x5b, 0x99],
      [0x5c, 0x83],
      [0x5d, 0x35],
      [0x5e, 0x82],
      [0x5f, 0x96],
      [0x60, 0x80],
      [0x61, 0x98],
      [0x62, 0x2e],
      [0xe0, 0x9c],
      [0xe1, 0x9a],
      [0xe2, 0x9e],
      [0xe4, 0x9d],
      [0xe5, 0x9b],
      [0xe6, 0x9f],
    ]);
    if (code >= 0x3a && code <= 0x45) {
      return 0x84 + code - 0x3a;
    }
    return keys.get(code);
  }

  class TranslatedPvmRuntime {
    constructor(module, assets, emit, maxGas, audioEnabled, gpuCapabilities = null) {
      this.metadata = readMetadata(module);
      this.instance = new WebAssembly.Instance(module, {});
      this.pvm = this.instance.exports;
      this.memories = [
        this.pvm.memory_ro,
        this.pvm.memory_rw,
        this.pvm.memory_stack,
      ];
      if (
        this.memories.some((memory) => !(memory instanceof WebAssembly.Memory))
      ) {
        throw new Error("translated PolkaVM module is missing guest memories");
      }
      this.assets = new Map(
        assets.map((asset) => [
          normalizedPath(asset.path),
          new Uint8Array(asset.bytes),
        ]),
      );
      this.emit = emit;
      this.audioEnabled = audioEnabled;
      this.gpuCapabilities =
        gpuCapabilities instanceof Uint8Array ? gpuCapabilities.slice() : null;
      this.gpuEvents = [];
      this.gpuSubmits = 0;
      this.gpuLastSequence = 0n;
      this.maxGas = BigInt(maxGas);
      this.input = [];
      this.coreInput = [];
      this.pointer = null;
      this.timeMs = null;
      this.clockStartedAt = performance.now();
      this.hostcalls = 0;
      this.hostcallBytes = 0;
      this.resumePending = false;
      this.stopped = false;
      this.coreVm = this.metadata.exports.has("_pvm_start");
      this.coreVmStarted = false;
      this.palette = new Uint32Array(256);
      this.palette.fill(0xffffffff);
      this.audioChannels = 0;
      this.audioSampleRate = 0;
      this.fds = new Map();
      this.nextFd = 3;
      this.imports = this.metadata.imports;
      this.exports = this.metadata.exports;
      for (let index = 0; index < 13; index++) {
        if (!(this.pvm[`r${index}`] instanceof WebAssembly.Global)) {
          throw new Error("translated PolkaVM module is missing registers");
        }
      }
    }

    initialize() {
      this.#resetBudget(MAX_HOSTCALLS_PER_INIT);
      if (this.coreVm) {
        this.#setupCoreVm();
        return;
      }
      const init = this.exports.get("init");
      if (init !== undefined) {
        this.#run(init, false);
      }
    }

    update(timeMs) {
      if (this.stopped) {
        return;
      }
      this.timeMs = timeMs;
      this.gpuSubmits = 0;
      this.#resetBudget(
        this.coreVm && !this.coreVmStarted
          ? MAX_HOSTCALLS_PER_INIT
          : MAX_HOSTCALLS_PER_UPDATE,
      );
      if (this.coreVm) {
        this.#run(this.exports.get("_pvm_start"), true);
        this.coreVmStarted = true;
        return;
      }
      const update = this.exports.get("update");
      if (update === undefined) {
        throw new Error("translated PolkaVM guest has no update export");
      }
      this.#run(update, false);
    }

    sendInput(bytes) {
      if (this.stopped || bytes.byteLength !== INPUT_EVENT_BYTES) {
        return;
      }
      if (!this.coreVm) {
        if (this.input.length === MAX_INPUT_EVENTS) {
          this.input.shift();
        }
        this.input.push(bytes.slice());
        return;
      }
      const view = new DataView(
        bytes.buffer,
        bytes.byteOffset,
        bytes.byteLength,
      );
      const type = bytes[0];
      if (type === 1 || type === 2) {
        const key = hidToCoreVm(bytes[1]);
        if (key !== undefined) {
          this.#queueCoreInput(key, type === 1 ? 1 : 0);
        }
      } else if (type === 3 || type === 4) {
        const key =
          bytes[1] >= 1 && bytes[1] <= 3 ? 0x9f + bytes[1] : undefined;
        if (key !== undefined) {
          this.#queueCoreInput(key, type === 3 ? 1 : 0);
        }
      } else if (type === 5) {
        const current = [view.getUint16(2, true), view.getUint16(4, true)];
        if (this.pointer) {
          this.#queueCoreInput(
            0xa3,
            Math.max(-128, Math.min(127, current[0] - this.pointer[0])) & 0xff,
          );
          this.#queueCoreInput(
            0xa4,
            Math.max(-128, Math.min(127, current[1] - this.pointer[1])) & 0xff,
          );
        }
        this.pointer = current;
      } else if (type === 6) {
        this.#queueCoreInput(
          0xa3,
          Math.max(-128, Math.min(127, view.getInt16(2, true))) & 0xff,
        );
        this.#queueCoreInput(
          0xa4,
          Math.max(-128, Math.min(127, view.getInt16(4, true))) & 0xff,
        );
      }
    }

    sendGpuEvent(bytes) {
      if (
        this.stopped ||
        !(bytes instanceof Uint8Array) ||
        bytes.byteLength < 24 ||
        bytes.byteLength > MAX_GPU_EVENT_BYTES ||
        decoder.decode(bytes.subarray(0, 4)) !== "EGE1"
      ) {
        return;
      }
      if (this.gpuEvents.length === MAX_GPU_EVENTS) {
        this.gpuEvents.shift();
      }
      this.gpuEvents.push(bytes.slice());
    }

    stop() {
      this.stopped = true;
      this.input.length = 0;
      this.coreInput.length = 0;
      this.gpuEvents.length = 0;
    }

    #resetBudget(hostcalls) {
      this.hostcalls = hostcalls;
      this.hostcallBytes = MAX_HOSTCALL_BYTES;
      this.pvm.pvm_set_gas(this.maxGas);
    }

    #run(entry, yieldOnFrame) {
      let status;
      if (this.resumePending) {
        this.resumePending = false;
        status = this.pvm.pvm_resume();
      } else {
        if (entry === undefined) {
          throw new Error("translated PolkaVM entrypoint is missing");
        }
        status = this.pvm.pvm_begin(entry, this.maxGas);
      }
      for (;;) {
        if (status === STATUS_FINISHED) {
          if (this.coreVm) {
            throw new Error("CoreVM guest exited");
          }
          return;
        }
        if (status === STATUS_TRAP) {
          throw new Error(
            `translated PolkaVM execution trapped at ${this.pvm.trap_pc.value}`,
          );
        }
        if (status === STATUS_OUT_OF_GAS) {
          throw new Error("translated PolkaVM guest ran out of gas");
        }
        if (status !== STATUS_ECALL) {
          throw new Error(
            `translated PolkaVM returned invalid status ${status}`,
          );
        }
        const importIndex = this.pvm.ecall.value >>> 0;
        const name = this.imports[importIndex];
        if (!name) {
          throw new Error(
            `translated PolkaVM called unknown import ${importIndex}`,
          );
        }
        if (--this.hostcalls < 0) {
          throw new Error(
            `translated PolkaVM guest exceeded hostcall budget in ${name}`,
          );
        }
        const yielded = this.coreVm
          ? this.#handleCoreVmCall(name)
          : this.#handleCooperativeCall(name);
        if (yielded && yieldOnFrame) {
          this.resumePending = true;
          return;
        }
        status = this.pvm.pvm_resume();
      }
    }

    #reg(index) {
      return BigInt.asUintN(64, this.pvm[`r${index}`].value);
    }

    #setReg(index, value) {
      const normalized = this.metadata.is64Bit
        ? BigInt.asIntN(64, BigInt(value))
        : BigInt.asUintN(32, BigInt(value));
      this.pvm[`r${index}`].value = normalized;
    }

    #u32(value) {
      return Number(value & 0xffffffffn) >>> 0;
    }

    #range(address, length, write = false) {
      address >>>= 0;
      length >>>= 0;
      const end = address + length;
      if (end > 0x100000000) {
        throw new Error(
          "translated PolkaVM guest memory access is out of range",
        );
      }
      const { layout } = this.metadata;
      let memory;
      let physical;
      if (address >= layout.stackLow && end <= layout.stackHigh) {
        memory = this.memories[2];
        physical = address - layout.stackLow;
      } else if (
        address >= layout.rwAddress &&
        end <= layout.rwAddress + this.memories[1].buffer.byteLength
      ) {
        memory = this.memories[1];
        physical = address - layout.rwAddress;
      } else if (
        !write &&
        address >= layout.roAddress &&
        end <= layout.roAddress + layout.roSize
      ) {
        memory = this.memories[0];
        physical = address - layout.roAddress;
      } else {
        throw new Error(
          "translated PolkaVM guest memory access is out of range",
        );
      }
      return new Uint8Array(memory.buffer, physical, length);
    }

    #read(address, length) {
      this.#chargeBytes(length);
      return this.#range(address, length).slice();
    }

    #write(address, bytes) {
      this.#chargeBytes(bytes.byteLength);
      this.#range(address, bytes.byteLength, true).set(bytes);
    }

    #chargeBytes(length) {
      if (
        !Number.isSafeInteger(length) ||
        length < 0 ||
        length > this.hostcallBytes
      ) {
        throw new Error(
          "translated PolkaVM guest exceeded hostcall byte budget",
        );
      }
      this.hostcallBytes -= length;
    }

    #readU64(address) {
      const bytes = this.#range(address >>> 0, 8);
      return new DataView(bytes.buffer, bytes.byteOffset, 8).getBigUint64(
        0,
        true,
      );
    }

    #writeU64(address, value) {
      const bytes = this.#range(address >>> 0, 8, true);
      new DataView(bytes.buffer, bytes.byteOffset, 8).setBigUint64(
        0,
        BigInt.asUintN(64, value),
        true,
      );
    }

    #readCString(address) {
      const output = [];
      for (let offset = 0; offset < 255; offset++) {
        let byte;
        try {
          byte = this.#range((address + offset) >>> 0, 1)[0];
        } catch {
          return null;
        }
        if (!byte) {
          return new Uint8Array(output);
        }
        output.push(byte);
      }
      return null;
    }

    #handleCooperativeCall(name) {
      const a0 = this.#reg(7);
      const a1 = this.#reg(8);
      const a2 = this.#reg(9);
      const a3 = this.#reg(10);
      const a4 = this.#reg(11);
      switch (name) {
        case "host_present_frame": {
          const width = this.#u32(a1);
          const height = this.#u32(a2);
          const stride = this.#u32(a3);
          const rowBytes = width * 4;
          const length = rowBytes * height;
          if (
            !width ||
            !height ||
            stride !== rowBytes ||
            length > MAX_FRAME_BYTES
          ) {
            this.#setReg(7, 1n);
            return false;
          }
          const source = this.#read(this.#u32(a0), length);
          const pixels = new Uint8Array(length);
          for (let index = 0; index < length; index += 4) {
            pixels[index] = source[index + 2];
            pixels[index + 1] = source[index + 1];
            pixels[index + 2] = source[index];
            pixels[index + 3] = source[index + 3];
          }
          this.emit({ type: "frame", width, height, pixels }, [pixels.buffer]);
          this.#setReg(7, 0n);
          return false;
        }
        case "host_tri2d_submit": {
          const length = this.#u32(a1);
          if (!length || length > MAX_TRI2D_BYTES) {
            this.#setReg(7, 1n);
            return false;
          }
          const bytes = this.#read(this.#u32(a0), length);
          this.emit({ type: "tri2d", bytes }, [bytes.buffer]);
          this.#setReg(7, 0n);
          return false;
        }
        case "epoca_gpu_capabilities": {
          if (this.gpuCapabilities === null) {
            this.#setReg(7, BigInt(GPU_ERROR_INVALID_STATE));
            return false;
          }
          const capacity = this.#u32(a1);
          const required = this.gpuCapabilities.byteLength;
          if (capacity < required) {
            this.#setReg(7, BigInt(-required));
            return false;
          }
          this.#write(this.#u32(a0), this.gpuCapabilities);
          this.#setReg(7, BigInt(required));
          return false;
        }
        case "epoca_gpu_submit": {
          const length = this.#u32(a1);
          if (this.gpuCapabilities === null) {
            this.#setReg(7, BigInt(GPU_ERROR_INVALID_STATE));
            return false;
          }
          if (
            !length ||
            length > MAX_GPU_BATCH_BYTES ||
            this.gpuSubmits === MAX_GPU_SUBMITS_PER_UPDATE
          ) {
            this.#setReg(
              7,
              BigInt(
                this.gpuSubmits === MAX_GPU_SUBMITS_PER_UPDATE
                  ? GPU_ERROR_QUOTA_EXCEEDED
                  : GPU_ERROR_MALFORMED_BATCH,
              ),
            );
            return false;
          }
          const bytes = this.#read(this.#u32(a0), length);
          const sequence = this.#gpuBatchSequence(bytes);
          if (sequence === null || sequence <= this.gpuLastSequence) {
            this.#setReg(7, BigInt(GPU_ERROR_MALFORMED_BATCH));
            return false;
          }
          this.gpuSubmits++;
          this.gpuLastSequence = sequence;
          this.emit({ type: "gpu-batch", bytes }, [bytes.buffer]);
          this.#setReg(7, 0n);
          return false;
        }
        case "epoca_gpu_receive": {
          const event = this.gpuEvents[0];
          if (event === undefined) {
            this.#setReg(7, 0n);
            return false;
          }
          const capacity = this.#u32(a1);
          if (capacity < event.byteLength) {
            this.#setReg(7, BigInt(-event.byteLength));
            return false;
          }
          this.#write(this.#u32(a0), event);
          this.gpuEvents.shift();
          this.#setReg(7, BigInt(event.byteLength));
          return false;
        }
        case "host_poll_input": {
          const capacity = this.#u32(a1);
          const count = Math.min(
            Math.floor(capacity / INPUT_EVENT_BYTES),
            this.input.length,
          );
          const output = new Uint8Array(count * INPUT_EVENT_BYTES);
          for (let index = 0; index < count; index++) {
            output.set(this.input.shift(), index * INPUT_EVENT_BYTES);
          }
          this.#write(this.#u32(a0), output);
          this.#setReg(7, BigInt(output.byteLength));
          return false;
        }
        case "host_time_ms": {
          const timeMs = this.timeMs ?? performance.now() - this.clockStartedAt;
          this.#setReg(7, BigInt(Math.max(0, Math.trunc(timeMs))));
          return false;
        }
        case "host_sleep_ms":
          this.timeMs =
            (this.timeMs ?? performance.now() - this.clockStartedAt) +
            Math.min(this.#u32(a0), 50);
          return false;
        case "host_audio_submit": {
          if (!this.audioEnabled) {
            this.#setReg(7, 3n);
            return false;
          }
          const sampleCount = this.#u32(a1);
          if (
            !sampleCount ||
            sampleCount % 2 ||
            sampleCount > MAX_AUDIO_SAMPLES
          ) {
            this.#setReg(7, 1n);
            return false;
          }
          const samples = this.#read(this.#u32(a0), sampleCount * 2);
          this.emit(
            { type: "audio", sampleRate: 48000, channels: 2, samples },
            [samples.buffer],
          );
          this.#setReg(7, 0n);
          return false;
        }
        case "host_asset_read": {
          const nameLength = this.#u32(a1);
          const offset = this.#u32(a2);
          const destination = this.#u32(a3);
          const capacity = this.#u32(a4);
          if (!nameLength || nameLength > 1024) {
            this.#setReg(7, 0n);
            return false;
          }
          const assetName = decoder.decode(
            this.#read(this.#u32(a0), nameLength),
          );
          const asset = this.assets.get(assetName);
          if (!asset || offset >= asset.byteLength) {
            this.#setReg(7, 0n);
            return false;
          }
          const length = Math.min(
            capacity,
            asset.byteLength - offset,
            16 * 1024 * 1024,
          );
          this.#write(destination, asset.subarray(offset, offset + length));
          this.#setReg(7, BigInt(length));
          return false;
        }
        case "host_save_submit": {
          const length = this.#u32(a1);
          if (!length || length > MAX_SAVE_BYTES) {
            this.#setReg(7, 1n);
            return false;
          }
          const bytes = this.#read(this.#u32(a0), length);
          this.emit({ type: "save", bytes }, [bytes.buffer]);
          this.#setReg(7, 0n);
          return false;
        }
        case "host_log": {
          const length = Math.min(this.#u32(a1), MAX_LOG_BYTES);
          const message = decoder.decode(this.#read(this.#u32(a0), length));
          this.emit({ type: "log", message });
          return false;
        }
        default:
          throw new Error(
            `translated PolkaVM guest uses unsupported import ${name}`,
          );
      }
    }

    #gpuBatchSequence(bytes) {
      if (
        bytes.byteLength < 24 ||
        decoder.decode(bytes.subarray(0, 4)) !== "EPG1"
      ) {
        return null;
      }
      const view = new DataView(
        bytes.buffer,
        bytes.byteOffset,
        bytes.byteLength,
      );
      if (
        view.getUint16(4, true) !== 1 ||
        view.getUint16(6, true) !== 0 ||
        view.getUint32(8, true) !== bytes.byteLength
      ) {
        return null;
      }
      const commandCount = view.getUint32(12, true);
      const sequence = view.getBigUint64(16, true);
      if (sequence === 0n || commandCount > MAX_GPU_COMMANDS) {
        return null;
      }
      let offset = 24;
      for (let index = 0; index < commandCount; index++) {
        if (
          offset + 8 > bytes.byteLength ||
          view.getUint16(offset + 2, true) !== 0
        ) {
          return null;
        }
        const commandBytes = view.getUint32(offset + 4, true);
        if (
          commandBytes < 8 ||
          commandBytes % 4 ||
          commandBytes > bytes.byteLength - offset
        ) {
          return null;
        }
        offset += commandBytes;
      }
      return offset === bytes.byteLength ? sequence : null;
    }

    #setupCoreVm() {
      let sp = BigInt(this.metadata.layout.stackHigh);
      const argc = 1n;
      sp -= (1n + argc + 1n + 0n + 1n + 4n) * 8n;
      const addressInit = sp;
      let pointer = sp;
      this.#writeU64(Number(pointer), argc);
      pointer += 8n;
      const argument = encoder.encode("./quake\0");
      sp -= BigInt(argument.byteLength);
      this.#write(Number(sp), argument);
      this.#writeU64(Number(pointer), sp);
      pointer += 16n;
      pointer += 8n;
      this.#writeU64(Number(pointer), 6n);
      this.#writeU64(Number(pointer + 8n), 4096n);
      this.#setReg(1, sp);
      this.#setReg(7, addressInit);
    }

    #queueCoreInput(key, value) {
      if (!value && (key === 0xa3 || key === 0xa4)) {
        return;
      }
      if (key === 0xa3 || key === 0xa4) {
        const existing = this.coreInput.find((event) => event[0] === key);
        if (existing) {
          existing[1] =
            Math.max(
              -128,
              Math.min(127, signedByte(existing[1]) + signedByte(value)),
            ) & 0xff;
          return;
        }
      }
      if (this.coreInput.length === 256) {
        this.coreInput.shift();
      }
      this.coreInput.push([key, value]);
    }

    #handleCoreVmCall(name) {
      switch (name) {
        case "pvm_set_palette": {
          const palette = this.#read(this.#u32(this.#reg(7)), 256 * 3);
          for (let index = 0; index < 256; index++) {
            const offset = index * 3;
            this.palette[index] =
              palette[offset] |
              (palette[offset + 1] << 8) |
              (palette[offset + 2] << 16) |
              0xff000000;
          }
          return false;
        }
        case "pvm_display": {
          const width = this.#u32(this.#reg(7));
          const height = this.#u32(this.#reg(8));
          const length = width * height;
          if (!width || !height || length > MAX_FRAME_BYTES / 4) {
            throw new Error("CoreVM guest supplied invalid frame dimensions");
          }
          const indices = this.#read(this.#u32(this.#reg(9)), length);
          const pixels = new Uint8Array(length * 4);
          const rgba = new Uint32Array(pixels.buffer);
          for (let index = 0; index < length; index++) {
            rgba[index] = this.palette[indices[index]];
          }
          this.emit({ type: "frame", width, height, pixels }, [pixels.buffer]);
          return true;
        }
        case "pvm_fetch_epoca_inputs":
        case "pvm_fetch_inputs": {
          const count = Math.min(
            this.#u32(this.#reg(8)),
            this.coreInput.length,
          );
          const output = new Uint8Array(count * 2);
          for (let index = 0; index < count; index++) {
            output.set(this.coreInput.shift(), index * 2);
          }
          this.#write(this.#u32(this.#reg(7)), output);
          this.#setReg(7, BigInt(count));
          return false;
        }
        case "pvm_asset_read": {
          const nameLength = this.#u32(this.#reg(8));
          const offset = this.#u32(this.#reg(9));
          const destination = this.#u32(this.#reg(10));
          const capacity = this.#u32(this.#reg(11));
          if (!nameLength || nameLength > 1024) {
            this.#setReg(7, 0n);
            return false;
          }
          const assetName = decoder.decode(
            this.#read(this.#u32(this.#reg(7)), nameLength),
          );
          const asset = this.assets.get(assetName);
          if (!asset || offset >= asset.byteLength) {
            this.#setReg(7, 0n);
            return false;
          }
          const length = Math.min(
            capacity,
            asset.byteLength - offset,
            16 * 1024 * 1024,
          );
          this.#write(destination, asset.subarray(offset, offset + length));
          this.#setReg(7, BigInt(length));
          return false;
        }
        case "host_audio_submit": {
          if (!this.audioEnabled) {
            this.#setReg(7, 3n);
            return false;
          }
          const sampleCount = this.#u32(this.#reg(8));
          if (
            !sampleCount ||
            sampleCount % 2 ||
            sampleCount > MAX_AUDIO_SAMPLES
          ) {
            this.#setReg(7, 1n);
            return false;
          }
          const samples = this.#read(this.#u32(this.#reg(7)), sampleCount * 2);
          this.emit(
            { type: "audio", sampleRate: 48000, channels: 2, samples },
            [samples.buffer],
          );
          this.#setReg(7, 0n);
          return false;
        }
        case "pvm_time_ms": {
          const timeMs = this.timeMs ?? performance.now() - this.clockStartedAt;
          this.#setReg(7, BigInt(Math.max(0, Math.trunc(timeMs))));
          return false;
        }
        case "host_log": {
          const length = Math.min(this.#u32(this.#reg(8)), MAX_LOG_BYTES);
          const message = decoder.decode(
            this.#read(this.#u32(this.#reg(7)), length),
          );
          this.emit({ type: "log", message });
          return false;
        }
        case "pvm_yield":
          return true;
        case "pvm_init_audio": {
          const channels = this.#u32(this.#reg(7));
          const bitsPerSample = this.#u32(this.#reg(8));
          const sampleRate = this.#u32(this.#reg(9));
          if (
            bitsPerSample !== 16 ||
            channels < 1 ||
            channels > 2 ||
            sampleRate < 8000 ||
            sampleRate > 96000
          ) {
            this.#setReg(7, 0n);
          } else {
            this.audioChannels = channels;
            this.audioSampleRate = sampleRate;
            this.#setReg(7, 1n);
          }
          return false;
        }
        case "pvm_output_audio": {
          const frames = this.#u32(this.#reg(8));
          const sampleCount = Math.min(frames * this.audioChannels, 1024 * 64);
          if (this.audioChannels && sampleCount) {
            const samples = this.#read(
              this.#u32(this.#reg(7)),
              sampleCount * 2,
            );
            this.emit(
              {
                type: "audio",
                sampleRate: this.audioSampleRate,
                channels: this.audioChannels,
                samples,
              },
              [samples.buffer],
            );
          }
          return false;
        }
        case "pvm_syscall":
          return this.#handleCoreVmSyscall();
        default:
          throw new Error(
            `translated CoreVM guest uses unsupported import ${name}`,
          );
      }
    }

    #handleCoreVmSyscall() {
      const syscall = this.#reg(7);
      const a1 = this.#reg(8);
      const a2 = this.#reg(9);
      const a3 = this.#reg(10);
      if (syscall === SYS_READ) {
        this.#setReg(7, this.#readFile(a1, a2, a3));
      } else if (syscall === SYS_READV || syscall === SYS_WRITEV) {
        if (!a3 || a3 > IOV_MAX) {
          this.#setReg(7, errno(EINVAL));
          return false;
        }
        let total = 0n;
        for (let index = 0n; index < a3; index++) {
          let address;
          let length;
          try {
            address = this.#readU64(this.#u32(a2 + index * 16n));
            length = this.#readU64(this.#u32(a2 + index * 16n + 8n));
          } catch {
            this.#setReg(7, errno(EFAULT));
            return false;
          }
          const result =
            syscall === SYS_READV
              ? this.#readFile(a1, address, length)
              : this.#writeFile(a1, address, length);
          if (BigInt.asIntN(64, result) < 0n) {
            this.#setReg(7, result);
            return false;
          }
          total += length;
        }
        this.#setReg(7, total);
      } else if (syscall === SYS_EXIT) {
        if (a1 === 0n) {
          throw new Error("CoreVM guest exited");
        }
        throw new Error(`CoreVM guest exited with status ${a1}`);
      } else if (syscall === SYS_OPENAT) {
        if (a1 !== AT_FDCWD) {
          this.#setReg(7, errno(ENOSYS));
          return false;
        }
        const pathBytes = this.#readCString(this.#u32(a2));
        if (!pathBytes) {
          this.#setReg(7, errno(EFAULT));
          return false;
        }
        const path = normalizedPath(decoder.decode(pathBytes));
        const flags = a3;
        const asset = this.assets.get(path);
        if (!asset) {
          this.#setReg(7, errno(ENOENT));
        } else if (flags & 3n) {
          this.#setReg(7, errno(EACCES));
        } else {
          const fd = this.nextFd++;
          this.fds.set(fd, { bytes: asset, position: 0n });
          this.#setReg(7, BigInt(fd));
        }
      } else if (syscall === SYS_LSEEK) {
        this.#setReg(7, this.#seekFile(a1, a2, a3));
      } else if (syscall === SYS_CLOSE) {
        const fd = this.#fdNumber(a1);
        if (fd === null || !this.fds.delete(fd)) {
          this.#setReg(7, errno(EBADF));
        } else {
          this.#setReg(7, 0n);
        }
      } else {
        this.#setReg(7, errno(ENOSYS));
      }
      return false;
    }

    #fdNumber(value) {
      return value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : null;
    }

    #readFile(fdValue, address, length) {
      const fd = this.#fdNumber(fdValue);
      const file = fd === null ? null : this.fds.get(fd);
      if (!file) {
        return errno(EBADF);
      }
      if (
        address + length > 0x100000000n ||
        length > BigInt(MAX_HOSTCALL_BYTES)
      ) {
        return errno(EFAULT);
      }
      const end =
        file.position + length < BigInt(file.bytes.byteLength)
          ? file.position + length
          : BigInt(file.bytes.byteLength);
      if (file.position >= end) {
        return 0n;
      }
      const count = Number(end - file.position);
      try {
        this.#write(
          this.#u32(address),
          file.bytes.subarray(
            Number(file.position),
            Number(file.position) + count,
          ),
        );
      } catch {
        return errno(EFAULT);
      }
      file.position += BigInt(count);
      return BigInt(count);
    }

    #writeFile(fdValue, address, length) {
      if (fdValue !== 1n && fdValue !== 2n) {
        return errno(EBADF);
      }
      if (
        address + length > 0x100000000n ||
        length > BigInt(MAX_HOSTCALL_BYTES)
      ) {
        return errno(EFAULT);
      }
      try {
        const byteLength = Number(length);
        this.#chargeBytes(byteLength);
        const bytes = this.#range(this.#u32(address), byteLength);
        const message = decoder.decode(bytes.subarray(0, MAX_LOG_BYTES));
        if (message) {
          this.emit({ type: "log", message });
        }
      } catch {
        return errno(EFAULT);
      }
      return length;
    }

    #seekFile(fdValue, offsetValue, whence) {
      const fd = this.#fdNumber(fdValue);
      const file = fd === null ? null : this.fds.get(fd);
      if (!file) {
        return errno(EBADF);
      }
      const offset = BigInt.asIntN(64, offsetValue);
      const fileLength = BigInt(file.bytes.byteLength);
      if (whence === 0n) {
        file.position = BigInt.asUintN(64, offset);
      } else if (whence === 1n) {
        file.position = BigInt.asUintN(
          64,
          BigInt.asIntN(64, file.position) + offset,
        );
        if (file.position > fileLength) {
          file.position = fileLength;
        }
      } else if (whence === 2n) {
        file.position = BigInt.asUintN(
          64,
          BigInt.asIntN(64, fileLength) + offset,
        );
        if (file.position > fileLength) {
          file.position = fileLength;
        }
      } else {
        return errno(EINVAL);
      }
      return file.position;
    }
  }

  globalThis.TranslatedPvmRuntime = TranslatedPvmRuntime;
})();

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

const FRAME_INTERVAL_MS = 1000 / 60;
const MAX_GAS_PER_UPDATE = 500_000_000;
const MAX_TRANSLATED_LOOPS_PER_UPDATE = 50_000_000;
const decoder = new TextDecoder();
const encoder = new TextEncoder();

let pvm;
let translated;
let backend = "interpreter";
let running = false;
let timer;
let startedAt = 0;
let updateCount = 0;
const updateSamples = [];
const tickChannel = new MessageChannel();
tickChannel.port1.onmessage = tick;

function errorText() {
  const pointer = pvm.pvm_browser_error_pointer();
  const length = pvm.pvm_browser_error_length();
  return decoder.decode(new Uint8Array(pvm.memory.buffer, pointer, length));
}

function check(status, operation) {
  if (status !== 0) {
    throw new Error(`${operation}: ${errorText()}`);
  }
}

function stage(bytes) {
  const source =
    bytes instanceof Uint8Array
      ? bytes
      : new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const pointer = pvm.pvm_browser_staging_reserve(source.byteLength);
  if (!pointer) {
    throw new Error(`reserve browser runtime memory: ${errorText()}`);
  }
  new Uint8Array(pvm.memory.buffer, pointer, source.byteLength).set(source);
}

function addAsset(asset) {
  const path = encoder.encode(asset.path.replace(/^\/+/, ""));
  const bytes = new Uint8Array(asset.bytes);
  const packed = new Uint8Array(path.byteLength + bytes.byteLength);
  packed.set(path);
  packed.set(bytes, path.byteLength);
  stage(packed);
  check(
    pvm.pvm_browser_launch_add_asset(path.byteLength),
    `mount browser asset ${asset.path}`,
  );
}

function drainFrame() {
  if (!pvm.pvm_browser_take_frame()) {
    return;
  }
  const width = pvm.pvm_browser_frame_width();
  const height = pvm.pvm_browser_frame_height();
  const length = pvm.pvm_browser_frame_length();
  const source = new Uint8Array(
    pvm.memory.buffer,
    pvm.pvm_browser_frame_pointer(),
    length,
  );
  const pixels = new Uint8Array(length);
  for (let index = 0; index < length; index += 4) {
    pixels[index] = source[index + 2];
    pixels[index + 1] = source[index + 1];
    pixels[index + 2] = source[index];
    pixels[index + 3] = source[index + 3];
  }
  postMessage({ type: "frame", width, height, pixels }, [pixels.buffer]);
}

function drainTri2d() {
  if (!pvm.pvm_browser_take_tri2d?.()) {
    return;
  }
  const length = pvm.pvm_browser_tri2d_length();
  const bytes = new Uint8Array(
    pvm.memory.buffer,
    pvm.pvm_browser_tri2d_pointer(),
    length,
  ).slice();
  postMessage({ type: "tri2d", bytes }, [bytes.buffer]);
}

function drainGpuBatches() {
  while (pvm.pvm_browser_take_gpu_batch?.()) {
    const length = pvm.pvm_browser_gpu_batch_length();
    const bytes = new Uint8Array(
      pvm.memory.buffer,
      pvm.pvm_browser_gpu_batch_pointer(),
      length,
    ).slice();
    postMessage({ type: "gpu-batch", bytes }, [bytes.buffer]);
  }
}

function drainAudio() {
  while (pvm.pvm_browser_take_audio()) {
    const sampleRate = pvm.pvm_browser_audio_sample_rate();
    const channels = pvm.pvm_browser_audio_channels();
    const length = pvm.pvm_browser_audio_length() * 2;
    const samples = new Uint8Array(
      pvm.memory.buffer,
      pvm.pvm_browser_audio_pointer(),
      length,
    ).slice();
    postMessage({ type: "audio", sampleRate, channels, samples }, [
      samples.buffer,
    ]);
  }
}

function drainSave() {
  while (pvm.pvm_browser_take_save()) {
    const length = pvm.pvm_browser_save_length();
    const bytes = new Uint8Array(
      pvm.memory.buffer,
      pvm.pvm_browser_save_pointer(),
      length,
    ).slice();
    postMessage({ type: "save", bytes }, [bytes.buffer]);
  }
}

function drainLogs() {
  while (pvm.pvm_browser_take_log()) {
    const pointer = pvm.pvm_browser_log_pointer();
    const length = pvm.pvm_browser_log_length();
    const message = decoder.decode(
      new Uint8Array(pvm.memory.buffer, pointer, length),
    );
    postMessage({ type: "log", message });
  }
}

function tick() {
  if (!running) {
    return;
  }
  const before = performance.now();
  try {
    if (translated) {
      translated.update(before - startedAt);
    } else {
      check(
        pvm.pvm_browser_update(before - startedAt),
        "update PolkaVM browser guest",
      );
      drainFrame();
      drainTri2d();
      drainGpuBatches();
      drainAudio();
      drainSave();
      drainLogs();
    }
  } catch (error) {
    running = false;
    postMessage({ type: "error", message: error.message });
    postMessage({ type: "terminated" });
    return;
  }
  const elapsed = performance.now() - before;
  updateCount++;
  updateSamples.push(elapsed);
  if (updateSamples.length > 600) {
    updateSamples.shift();
  }
  if (updateCount % 120 === 0) {
    const sorted = [...updateSamples].sort((a, b) => a - b);
    const percentile = (value) =>
      sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * value))];
    postMessage({
      type: "metrics",
      updates: updateCount,
      updateP50Ms: percentile(0.5),
      updateP95Ms: percentile(0.95),
      updateMaxMs: sorted[sorted.length - 1],
    });
  }
  const delay = FRAME_INTERVAL_MS - elapsed;
  if (delay > 0) {
    timer = setTimeout(tick, delay);
  } else {
    tickChannel.port2.postMessage(null);
  }
}

async function start(message) {
  if (pvm || running) {
    throw new Error("PolkaVM browser worker is already started");
  }
  const bootStarted = performance.now();
  let translationMs = 0;
  let compilationMs = 0;
  let translatedWasmBytes = 0;
  let cacheHit = false;
  const instantiated = await WebAssembly.instantiate(message.runtime, {});
  pvm = instantiated.instance.exports;
  if (pvm.pvm_browser_abi_version() !== 1) {
    throw new Error("PolkaVM browser runtime has an incompatible ABI");
  }
  const program = new Uint8Array(message.program);
  stage(program);
  const pendingOutputs = [];
  try {
    let module = message.compiledModule;
    let bytes =
      message.compiledBytes instanceof ArrayBuffer
        ? new Uint8Array(message.compiledBytes)
        : null;
    cacheHit = module instanceof WebAssembly.Module || bytes !== null;
    if (!(module instanceof WebAssembly.Module)) {
      if (bytes === null) {
        const translationStarted = performance.now();
        check(
          pvm.pvm_browser_translate_staged(),
          "translate PolkaVM browser guest",
        );
        translationMs = performance.now() - translationStarted;
        const pointer = pvm.pvm_browser_translation_pointer();
        const length = pvm.pvm_browser_translation_length();
        bytes = new Uint8Array(pvm.memory.buffer, pointer, length).slice();
        const persistent = bytes.slice();
        postMessage(
          { type: "translated", cacheKey: message.cacheKey, bytes: persistent },
          [persistent.buffer],
        );
      }
      translatedWasmBytes = bytes.byteLength;
      const compilationStarted = performance.now();
      module = await WebAssembly.compile(bytes);
      compilationMs = performance.now() - compilationStarted;
      try {
        postMessage({ type: "compiled", cacheKey: message.cacheKey, module });
      } catch {}
    }
    translated = new globalThis.TranslatedPvmRuntime(
      module,
      message.assets,
      (output, transfers = []) => {
        if (running) {
          postMessage(output, transfers);
        } else {
          pendingOutputs.push({ output, transfers });
        }
      },
      MAX_TRANSLATED_LOOPS_PER_UPDATE,
      message.audioEnabled,
      message.gpuCapabilities instanceof ArrayBuffer
        ? new Uint8Array(message.gpuCapabilities)
        : null,
    );
    translated.initialize();
    backend = "compiler";
  } catch (error) {
    translated = null;
    pendingOutputs.length = 0;
    console.warn(`PolkaVM translation failed; using interpreter: ${error}`);
    const presentation =
      message.graphicsProfile === "tri2d"
        ? 1
        : message.graphicsProfile === "webgpu-raster"
          ? 2
          : 0;
    const begin = pvm.pvm_browser_launch_begin_v2;
    if (typeof begin !== "function") {
      throw new Error("PolkaVM interpreter does not support graphics profiles");
    }
    stage(program);
    check(
      begin(
        MAX_GAS_PER_UPDATE,
        message.audioEnabled ? 1 : 0,
        presentation,
      ),
      "begin PolkaVM browser launch",
    );
    for (const asset of message.assets) {
      addAsset(asset);
    }
    check(pvm.pvm_browser_launch_start(), "start PolkaVM browser launch");
    if (message.graphicsProfile === "webgpu-raster") {
      if (!(message.gpuCapabilities instanceof ArrayBuffer)) {
        throw new Error("WebGPU capabilities are required before PVM initialization");
      }
      stage(new Uint8Array(message.gpuCapabilities));
      check(
        pvm.pvm_browser_set_gpu_capabilities(),
        "set PolkaVM browser GPU capabilities",
      );
    }
    try {
      check(pvm.pvm_browser_init(), "initialize PolkaVM browser guest");
    } catch (initError) {
      drainLogs();
      throw initError;
    }
    drainTri2d();
    drainGpuBatches();
    drainLogs();
  }
  startedAt = performance.now();
  running = true;
  postMessage({
    type: "ready",
    backend,
    cacheHit,
    translationMs,
    compilationMs,
    translatedWasmBytes,
    startupMs: performance.now() - bootStarted,
  });
  for (const { output, transfers } of pendingOutputs) {
    postMessage(output, transfers);
  }
  tick();
}

function sendInput(bytes) {
  if (!running || bytes.byteLength !== 8) {
    return;
  }
  if (translated) {
    translated.sendInput(bytes);
    return;
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  check(
    pvm.pvm_browser_send_input(
      bytes[0],
      bytes[1],
      view.getUint16(2, true),
      view.getUint16(4, true),
    ),
    "send PolkaVM browser input",
  );
}

function sendGpuEvent(bytes) {
  if (!running || !pvm || !bytes.byteLength) {
    return;
  }
  if (translated) {
    translated.sendGpuEvent(bytes);
    return;
  }
  stage(bytes);
  check(pvm.pvm_browser_send_gpu_event(), "send PolkaVM browser GPU event");
}

onmessage = (event) => {
  const message = event.data;
  if (message?.type === "start") {
    void start(message).catch((error) => {
      running = false;
      postMessage({ type: "error", message: error.message });
      postMessage({ type: "terminated" });
    });
  } else if (message?.type === "input") {
    try {
      sendInput(new Uint8Array(message.bytes));
    } catch (error) {
      running = false;
      clearTimeout(timer);
      postMessage({ type: "error", message: error.message });
      postMessage({ type: "terminated" });
    }
  } else if (message?.type === "gpu-event") {
    try {
      sendGpuEvent(new Uint8Array(message.bytes));
    } catch (error) {
      running = false;
      clearTimeout(timer);
      postMessage({ type: "error", message: error.message });
      postMessage({ type: "terminated" });
    }
  } else if (message?.type === "stop") {
    running = false;
    clearTimeout(timer);
    translated?.stop();
    pvm?.pvm_browser_reset();
    postMessage({ type: "terminated" });
  }
};
