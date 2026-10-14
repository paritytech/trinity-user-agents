# Native container permission boundary

The container runs before product code in the same JavaScript realm. Its private SDK client sends permission requests to
the host, and the browser API gates use the decoded host decision before calling captured native APIs. Product code must
not replace the operations that encode those requests, decode replies, or deliver private callbacks.

`freezePermissionRuntime` protects those shared operations while allowing React promise subclasses, Next/webpack queues,
and Buffer-style objects to assign their own methods. Assignments through these protected setters to frozen receivers
are ignored, including a frozen prototype inheriting a protected method from another prototype. Assignments on
extensible product objects create ordinary own properties. Explicit redefinition of a protected property with
`Object.defineProperty` can still throw.

## Required protections

- `Object` static methods build decoded records. `Object.prototype` cannot gain properties, preventing inherited `then`
  hooks and setters for decoded permission fields.
- Arrays, maps, weak maps, sets, and their iterators carry decoded values, pending resolvers, private SDK state, and
  reply listeners. Promise and SDK Result methods carry asynchronous permission decisions.
- `String`, `Number`, `Uint8Array`, and `DataView` protect request IDs, numeric validation, and encoded request/reply
  bytes. The global `BigInt` conversion and timeout functions are also pinned.
- `MessageChannel`, `MessagePort`, and `EventTarget` remain protected. The
  [legacy port getter](../packages/truapi/src/host-connection.ts) creates its channel lazily, then reads port methods
  and `addEventListener` after the handshake. Capturing native WebSocket operations at startup does not protect this
  path.
- Text encoder/decoder prototypes remain protected because SCALE codecs retain their instances but read their methods on
  each call.

## Built-ins that remain mutable

These choices follow the private permission path, not the absence of a successful attack in a test matrix. New deferred
lookups, different codecs, or dependency updates require checking this boundary again.

| Built-in                                   | Why it is not frozen                                                                                                                                                                                                                                                                                                                |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Function` and its prototype               | The [gates](src/network.ts) and [host connection](../packages/truapi/src/host-connection.ts) bind native operations during setup. The private SDK request path uses `ResultAsync.fromSafePromise`, `andThen`, and `then`, which do not use neverthrow's `Function.apply`-dependent generator helpers.                               |
| `Reflect`                                  | Gates and the [socket provider factory](../packages/truapi/src/transport.ts) capture the operations they use before product execution, including for reconnects.                                                                                                                                                                    |
| `Symbol`                                   | Explicit iterator keys are read during setup. Later private collection iteration uses the language's well-known iterator symbol and protected prototypes, not the current global `Symbol` binding. Public SDK iterator helpers are outside this claim.                                                                              |
| `Date`                                     | Connection freshness uses a bound `performance.now`; request deadlines use pinned timeout functions.                                                                                                                                                                                                                                |
| `MessageEvent`                             | The socket provider captures its native `data` getter before product code. The legacy adapter reads product-controlled event data, but rejects reserved `host:` request IDs. Private replies go directly from the socket provider to the private transport. Replacing this getter can still disrupt a product's own legacy traffic. |
| Existing `Object.prototype` methods        | The prototype cannot grow. Permission fields are own properties created by protected `Object.fromEntries`, and the gate reads the decoded own `granted` boolean. Existing inherited conversion methods are not invoked on private permission state.                                                                                 |
| `Number.prototype`                         | IDs interpolate primitive counters; codecs use primitive conversion, arithmetic, and protected `DataView` methods. The `Number` constructor and static validation methods remain protected.                                                                                                                                         |
| `ArrayBuffer`                              | The socket provider and browser gates capture the native buffer operations they need. Private permission and handshake replies use boolean/void/string codecs. Arbitrary public byte-valued SDK results can still depend on mutable buffer methods.                                                                                 |
| `TextEncoder` / `TextDecoder` constructors | SCALE creates its retained instances during module initialization. Their prototypes remain protected for subsequent method calls.                                                                                                                                                                                                   |
| `BigInt` prototype and static methods      | Private compact codecs use primitive arithmetic and the pinned global conversion, not prototype methods or `asUintN`.                                                                                                                                                                                                               |

[Authorization](src/network-transport.ts) reads the internal client, not public product methods. The fetch gate captures
native `fetch`, `Request`, and `Reflect.apply` during installation. Those captures are independent of the prototype
locking helper.

The tests exercise actual host denial with no native network request, positive grants after mutation and reconnect,
protection of the lazily created legacy endpoint, and ordinary library overrides. Run them with
`npm test --prefix js/container`. A passing test demonstrates its scenario; the source dependencies above determine
which locks are needed.
