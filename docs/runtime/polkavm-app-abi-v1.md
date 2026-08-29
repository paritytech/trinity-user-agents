---
title: "PolkaVM application runtime ABI v1"
type: runtime-contract
status: draft
---

# PolkaVM application runtime ABI v1

## Scope

This document defines the application-visible boundary selected by an App v2
manifest with:

```json
{
  "runtime": {
    "kind": "polkavm",
    "abiVersion": 1
  }
}
```

It covers cooperative PolkaVM applications with `init` and `update` exports,
the Host imports available to those applications, guest-memory rules, common
resource bounds, and failure behavior.

Graphics command payloads are defined by the separately versioned Framebuffer,
Tri2D, and WebGPU Raster profile contracts. The `_pvm_start` CoreVM
compatibility path is outside this ABI and must be specified separately before
it is advertised as a portable Product runtime.

## Conformance language

The key words MUST, MUST NOT, REQUIRED, SHOULD, SHOULD NOT, and MAY are to be
interpreted as described in RFC 2119.

A Host advertises PolkaVM application ABI v1 only when its observable behavior
conforms to this document and the conformance fixtures associated with it.

## Program model

The executable is a valid PolkaVM program selected by the App manifest's
archive-relative `runtime.entrypoint`.

The program MUST export:

```text
init() -> ()
update() -> ()
```

The Host instantiates a fresh program, calls `init` exactly once, and calls
`update` zero or more times while the App is running. Calls are serialized; the
Host MUST NOT enter the same program concurrently.

The Host selects and enforces a nonzero gas budget for each call. A trap, gas
exhaustion, invalid guest-memory access, or Host-call budget failure fails the
current execution. ABI v1 does not restart a failed program transparently.

The Host owns scheduling and presentation. Returning from `update` yields
control to the Host; it does not imply that a frame was presented.

## Byte order and guest memory

All integer and sample encodings defined by this ABI are little-endian.
Pointers are `u32` offsets into guest memory. A pointer is valid only for the
duration of the Host call receiving it. The Host MUST copy or consume the
referenced bytes before returning and MUST NOT retain guest pointers.

A Host MUST bounds-check every guest read and write. Integer overflow while
computing a range is an invalid guest-memory access. A failed memory access
fails the current execution unless an individual Host call explicitly defines
a returned status for that condition.

## Capability gating

The App manifest selects exactly one graphics profile and may enable device
input and audio. A Host call made outside its declared capability MUST fail
with that call's unavailable or invalid-state result. The Host MUST NOT
silently reinterpret a submission as another graphics profile.

## Host imports

### Framebuffer presentation

```text
host_present_frame(
  pointer: u32,
  width: u32,
  height: u32,
  stride: u32
) -> u32
```

The call submits one complete packed framebuffer. `stride` MUST equal
`width * 4`. The selected graphics profile MUST be `framebuffer`.

Return values:

```text
0  accepted
1  invalid dimensions, stride, or byte length
3  framebuffer profile unavailable for this execution
```

The Framebuffer profile contract defines pixel order, dimensions, and
presentation semantics.

### Tri2D presentation

```text
host_tri2d_submit(pointer: u32, length: u32) -> u32
```

The call submits one complete Tri2D command stream. The selected graphics
profile MUST be `tri2d`. ABI v1 accepts at most one Tri2D submission during one
`init` or `update` call.

Return values:

```text
0  accepted
1  malformed or out-of-bounds command stream
2  a Tri2D stream was already submitted during this call
3  Tri2D profile unavailable for this execution
```

The Tri2D profile contract defines the command stream and retained-resource
semantics.

### WebGPU Raster capabilities

```text
host_gpu_capabilities(pointer: u32, capacity: u32) -> i32
```

The selected graphics profile MUST be `webgpu-raster`. The Host writes the
current WebGPU Raster capability record when the supplied capacity is
sufficient.

Return values:

```text
> 0  capability-record bytes written
  0  capabilities are not ready
< 0  required capacity, represented as the negated byte count, or a stable
     GPU error defined by the WebGPU Raster contract
```

### WebGPU Raster submission

```text
host_gpu_submit(pointer: u32, length: u32) -> i32
```

The call submits one complete WebGPU Raster batch. Acceptance means that the
batch passed synchronous Host validation and was queued; it does not imply
shader compilation or GPU completion.

Return values are defined by the WebGPU Raster contract. ABI v1 reserves:

```text
 0  accepted
 1  bounded backpressure; the guest may retry
-1  invalid guest range
-2  malformed batch
-3  quota exceeded
-4  invalid or stale resource handle
-5  invalid lifecycle or profile state
-6  stopped execution
```

### WebGPU Raster events

```text
host_gpu_receive(pointer: u32, capacity: u32) -> i32
```

The call reads the oldest queued WebGPU Raster event.

```text
> 0  event bytes written
  0  no event is available
< 0  required capacity, represented as the negated byte count, or a stable
     GPU error defined by the WebGPU Raster contract
```

### Input

```text
host_poll_input(pointer: u32, capacity: u32) -> u32
```

The Host writes as many complete eight-byte input records as fit in `capacity`
and returns the number of bytes written. It never writes a partial record.
Zero means that no event was available or that the capacity was smaller than
one record.

An input record is:

```text
offset  type  field
0       u8    event type
1       u8    code
2       u16   x
4       u16   y
6       u16   zero
```

ABI v1 event types are:

```text
1  key down
2  key up
3  pointer button down
4  pointer button up
5  pointer position
6  pointer delta
7  surface metrics
```

The device-input contract defines code values, coordinate interpretation, and
surface-metric scaling. ABI v1 does not define touch, wheel, UTF-8 text, IME,
or focus events.

### Time

```text
host_time_ms() -> u64
host_sleep_ms(duration_ms: u32) -> ()
```

`host_time_ms` returns a monotonic millisecond clock scoped to the execution.
It is not wall-clock time.

`host_sleep_ms` yields or advances runtime time by no more than the remaining
sleep allowance for the current call. A Host MAY return earlier than the
requested duration.

### Audio

```text
host_audio_submit(pointer: u32, sample_count: u32) -> u32
```

Samples are interleaved signed 16-bit little-endian PCM, stereo, at 48,000 Hz.
`sample_count` counts individual channel samples and MUST therefore be even.

Return values:

```text
0  accepted
1  invalid sample count or audio queue limit reached
3  audio capability unavailable for this execution
```

### Assets

```text
host_asset_read(
  name_pointer: u32,
  name_length: u32,
  offset: u32,
  destination: u32,
  capacity: u32
) -> u32
```

The asset name is UTF-8 and relative to the verified application archive. The
Host writes at most `capacity` bytes starting at `offset` and returns the
number written.

Zero means the name was invalid, the asset was absent, or the offset was at or
past the end of the asset. Assets are immutable for the lifetime of one
execution.

### Save data

```text
host_save_submit(pointer: u32, length: u32) -> u32
```

The call submits one opaque save-data value for Host persistence. A later
successful submission replaces the pending value.

```text
0  accepted
1  empty or over the size limit
```

Storage lifetime, synchronization, and user controls are Host policy outside
this ABI.

### Logging

```text
host_log(pointer: u32, length: u32) -> ()
```

The Host copies at most the v1 log-byte limit and decodes the bytes as lossy
UTF-8 for diagnostics. Logs are not application storage and MUST NOT affect
application behavior.

## ABI v1 resource bounds

The initial v1 implementation applies the following ceilings:

```text
program bytes                         16 MiB
read-write data                       64 MiB
stack                                 16 MiB
heap                                  128 MiB
asset files                           2,048
one asset                             64 MiB
all assets                            128 MiB
one asset read                        16 MiB
Host-call bytes per init/update       32 MiB
Host calls during init                131,072
Host calls during update              8,192
sleep during init                     100 ms
sleep during update                   50 ms
audio samples per submission          96,000
queued audio                           2 seconds
queued input events                   4,096
save data                             1 MiB
one log                               4 KiB
queued logs                           64
queued GPU batches                    4
queued GPU events                     256
GPU submissions per init/update       8
GPU inline uploads per init/update    16 MiB
```

Profile contracts define their additional bounds. Conforming Hosts MUST NOT
accept values above these ceilings. Before this draft becomes stable, the Host
SDK maintainers must decide which values are also minimum capacities that every
conforming Host must provide.

## Failure and shutdown

A successful `init` does not guarantee that later updates will succeed. The
Host stops the execution on an unhandled guest trap, gas exhaustion, invalid
memory access, unrecoverable profile error, or Host transport failure.

The Host may stop an execution when its App surface closes, the Product is
replaced, or platform lifecycle policy requires termination. ABI v1 does not
promise transparent restoration of guest memory or graphics resources after a
stop.

Device loss and recoverable WebGPU Raster errors are delivered according to
the WebGPU Raster event contract. They do not permit stale resource handles to
be reused.

## Version compatibility

A Host that does not implement PolkaVM application ABI v1 MUST NOT launch an
App requesting it. A program compiled for ABI v1 imports only the symbols and
uses only the behavior defined by this document and its selected capability
contracts.

Changes to an import signature, lifecycle requirement, record layout, or
observable status meaning require a new ABI version unless explicitly defined
as a backward-compatible extension.

## Conformance

The normative fixture set contains reproducible PolkaVM guests and expected
results covering:

- required exports and initialization;
- repeated updates;
- guest traps and gas exhaustion;
- invalid guest-memory ranges;
- input record delivery;
- monotonic time;
- asset reads;
- audio submission and gating;
- save submission;
- bounded logging;
- graphics-profile enforcement.

Native and browser implementations MUST run the same fixture inputs. Full
sample applications are integration evidence rather than normative fixtures.

## Open questions before stabilization

- Which resource values are required minimum capacities across all Hosts?
- What stable registry defines key and pointer-button codes?
- Is `host_sleep_ms` necessary in the stable cooperative ABI, or should the
  Host own all scheduling without a guest sleep operation?
- Should save persistence be a capability declared separately from the base
  ABI?
- How is the CoreVM compatibility path named and versioned independently from
  this cooperative ABI?
