/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

/**
 * Creates the bounded PolkaVM endpoint in a Worker or on a host thread.
 *
 * @param {DedicatedWorkerGlobalScope} endpoint - Message endpoint owned by the runtime.
 */
globalThis.createPvmRuntime = (endpoint) => {
  const postMessage = (message, transfers) => {
    if (transfers) {
      endpoint.postMessage(message, transfers);
    } else {
      endpoint.postMessage(message);
    }
  };

  const FRAME_INTERVAL_MS = 1000 / 60;
  const MAX_GAS_PER_UPDATE = 500_000_000;
  const MAX_TRANSLATED_LOOPS_PER_UPDATE = 50_000_000;
  const MAX_PROGRAM_BYTES = 16 * 1024 * 1024;
  const MAX_ASSET_FILES = 2048;
  const MAX_ASSET_NAME_BYTES = 1024;
  const MAX_ASSET_FILE_BYTES = 64 * 1024 * 1024;
  const MAX_ASSET_BYTES = 128 * 1024 * 1024;
  const decoder = new TextDecoder();
  const encoder = new TextEncoder();

  let pvm;
  let translated;
  let backend = "interpreter";
  let running = false;
  let disposed = false;
  let timer;
  let startedAt = 0;
  let updateCount = 0;
  const updateSamples = [];
  const tickChannel = new MessageChannel();
  tickChannel.port1.onmessage = tick;

  function stopRuntime() {
    if (disposed) {
      return;
    }
    disposed = true;
    running = false;
    clearTimeout(timer);
    translated?.stop();
    pvm?.pvm_browser_reset?.();
    tickChannel.port1.close();
    tickChannel.port2.close();
    endpoint.onmessage = null;
  }

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
      stopRuntime();
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

  function asBytes(value, label) {
    if (value instanceof ArrayBuffer) {
      return new Uint8Array(value);
    }
    if (ArrayBuffer.isView(value)) {
      return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
    }
    throw new Error(`${label} must be binary data`);
  }

  function validateAssetPath(path) {
    const encoded = encoder.encode(path);
    if (
      !path ||
      encoded.byteLength > MAX_ASSET_NAME_BYTES ||
      path.startsWith("/") ||
      path.includes("\\") ||
      path
        .split("/")
        .some(
          (component) => !component || component === "." || component === "..",
        )
    ) {
      throw new Error(`invalid PolkaVM browser asset path ${path}`);
    }
  }

  function validateStartMessage(message) {
    if (
      !(message.runtime instanceof WebAssembly.Module) &&
      asBytes(message.runtime, "PolkaVM browser runtime").byteLength === 0
    ) {
      throw new Error("PolkaVM browser runtime is empty");
    }
    const program = asBytes(message.program, "PolkaVM browser program");
    if (!program.byteLength || program.byteLength > MAX_PROGRAM_BYTES) {
      throw new Error(
        `PolkaVM browser program must contain 1..=${MAX_PROGRAM_BYTES} bytes`,
      );
    }
    if (
      !["framebuffer", "tri2d", "webgpu-raster"].includes(
        message.graphicsProfile,
      )
    ) {
      throw new Error(
        `invalid PolkaVM browser graphics profile ${message.graphicsProfile}`,
      );
    }
    if (
      !Array.isArray(message.assets) ||
      message.assets.length > MAX_ASSET_FILES
    ) {
      throw new Error(
        `PolkaVM browser launch exceeds ${MAX_ASSET_FILES} assets`,
      );
    }
    const paths = new Set();
    let assetBytes = 0;
    for (const asset of message.assets) {
      if (!asset || typeof asset.path !== "string") {
        throw new Error("PolkaVM browser asset is missing its path");
      }
      validateAssetPath(asset.path);
      if (paths.has(asset.path)) {
        throw new Error(
          `PolkaVM browser asset path is duplicated: ${asset.path}`,
        );
      }
      paths.add(asset.path);
      const length = asBytes(
        asset.bytes,
        `PolkaVM browser asset ${asset.path}`,
      ).byteLength;
      if (length > MAX_ASSET_FILE_BYTES) {
        throw new Error(
          `PolkaVM browser asset ${asset.path} exceeds ${MAX_ASSET_FILE_BYTES} bytes`,
        );
      }
      assetBytes += length;
      if (!Number.isSafeInteger(assetBytes) || assetBytes > MAX_ASSET_BYTES) {
        throw new Error(
          `PolkaVM browser assets exceed ${MAX_ASSET_BYTES} bytes`,
        );
      }
    }
    if (
      message.graphicsProfile === "webgpu-raster" &&
      !(message.gpuCapabilities instanceof ArrayBuffer)
    ) {
      throw new Error(
        "WebGPU capabilities are required before PVM initialization",
      );
    }
    return program;
  }

  async function start(message) {
    if (disposed) {
      throw new Error("PolkaVM browser worker is stopped");
    }
    if (pvm || running) {
      throw new Error("PolkaVM browser worker is already started");
    }
    const program = validateStartMessage(message);
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
            {
              type: "translated",
              cacheKey: message.cacheKey,
              bytes: persistent,
            },
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
        message.graphicsProfile,
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
      let presentation = 0;
      if (message.graphicsProfile === "tri2d") {
        presentation = 1;
      } else if (message.graphicsProfile === "webgpu-raster") {
        presentation = 2;
      }
      const begin = pvm.pvm_browser_launch_begin_v2;
      if (typeof begin !== "function") {
        throw new Error(
          "PolkaVM interpreter does not support graphics profiles",
        );
      }
      stage(program);
      check(
        begin(MAX_GAS_PER_UPDATE, message.audioEnabled ? 1 : 0, presentation),
        "begin PolkaVM browser launch",
      );
      for (const asset of message.assets) {
        addAsset(asset);
      }
      check(pvm.pvm_browser_launch_start(), "start PolkaVM browser launch");
      if (message.graphicsProfile === "webgpu-raster") {
        if (!(message.gpuCapabilities instanceof ArrayBuffer)) {
          throw new Error(
            "WebGPU capabilities are required before PVM initialization",
          );
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

  endpoint.onmessage = (event) => {
    const message = event.data;
    if (message?.type === "start") {
      void start(message).catch((error) => {
        stopRuntime();
        postMessage({ type: "error", message: error.message });
        postMessage({ type: "terminated" });
      });
    } else if (message?.type === "input") {
      try {
        sendInput(new Uint8Array(message.bytes));
      } catch (error) {
        stopRuntime();
        postMessage({ type: "error", message: error.message });
        postMessage({ type: "terminated" });
      }
    } else if (message?.type === "gpu-event") {
      try {
        sendGpuEvent(new Uint8Array(message.bytes));
      } catch (error) {
        stopRuntime();
        postMessage({ type: "error", message: error.message });
        postMessage({ type: "terminated" });
      }
    } else if (message?.type === "stop") {
      stopRuntime();
      postMessage({ type: "terminated" });
    }
  };
};
