# @parity/truapi-debugger

## 0.1.10

### Patch Changes

- Updated dependencies [7663ece]
  - @parity/truapi@0.23.0

## 0.1.9

### Patch Changes

- Updated dependencies [a285523]
- Updated dependencies [60940ff]
- Updated dependencies [de343d4]
- Updated dependencies [3ef2191]
  - @parity/truapi@0.22.0

## 0.1.8

### Patch Changes

- 021815d: The debugger is marked private, which is what it already was in practice: it has never been published, and
  its README runs it from the workspace. Changesets bumped its version with every release because it depends on the
  client, so the version it declared climbed while nothing shipped, and the registry drift check reported a published
  version that was missing. Publishing it later is removing that one field.
- Updated dependencies [ffdd9b4]
- Updated dependencies [5a9f4b9]
- Updated dependencies [cf1702f]
  - @parity/truapi@0.21.0

## 0.1.7

### Patch Changes

- Updated dependencies [5f3dc71]
- Updated dependencies
  - @parity/truapi@0.20.0

## 0.1.6

### Patch Changes

- Updated dependencies [a78d73c]
- Updated dependencies [303b163]
- Updated dependencies [5d5fd1c]
  - @parity/truapi@0.19.0

## 0.1.5

### Patch Changes

- d3ec891: Request cancellation. Every generated request method takes `options?: CallOptions` last; aborting its
  `signal` sends a `Cancel` frame on that method's own address, correlated by the same `requestId`. The call still
  settles with exactly one response: a withdrawn call answers `CallError.Cancelled`, and a cancel that arrives too late
  is dropped so the real result stands. The client's own deadline sends `Cancel` before it rejects.

  Additive on the wire. `CallError` gains `Cancelled` as its last variant, so every existing discriminant keeps its
  SCALE index, and `WIRE_CODEC_VERSION` is unchanged. A host that predates the `Cancel` leg drops the frame with no
  reply and the call settles on the client's deadline instead. A product cannot detect that first, so an abort such a
  host never understood is indistinguishable from one it honoured.

- Updated dependencies [d9eaece]
- Updated dependencies [9b54ceb]
- Updated dependencies [33f9222]
- Updated dependencies [c2e5674]
- Updated dependencies [a63b0e8]
- Updated dependencies [448c1d4]
- Updated dependencies [d3ec891]
- Updated dependencies [e9c45b8]
- Updated dependencies [ea5e2f9]
  - @parity/truapi@0.18.0

## 0.1.4

### Patch Changes

- Updated dependencies [4034118]
- Updated dependencies [221d972]
- Updated dependencies [befde58]
- Updated dependencies [75a9f2e]
- Updated dependencies [cc1823d]
- Updated dependencies [0e86a3a]
- Updated dependencies [a9731b1]
- Updated dependencies [a9731b1]
- Updated dependencies [a9731b1]
  - @parity/truapi@0.17.0

## 0.1.3

### Patch Changes

- Updated dependencies [4c20296]
- Updated dependencies [4a93ac4]
- Updated dependencies [9252985]
  - @parity/truapi@0.16.0

## 0.1.2

### Patch Changes

- Updated dependencies [d36911f]
- Updated dependencies [e8ee375]
  - @parity/truapi@0.15.0

## 0.1.1

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.14.0
