# Chain Integration

How the app talks to substrate chains: storage, runtime APIs, SCALE codec, and chain registry. This doc is about the **API surface a feature uses**; for extrinsic submission see `transactions.md`.

## Rules at a glance

1. **`blocking`** — Editing existing hex in a SCALE conformance test is forbidden — that's the canary for accidental schema breakage.
2. **`blocking`** — Manual binary serializer for a new type when `@Serializable` / kotlinx-serialization SCALE works.
3. **`major`** — New `QueryableStorageEntry` declared with `binding = ::bindXxx` (legacy) instead of the reified `storageN<T>(name)` form. Make the value `@Serializable`.
4. **`major`** — Hand-rolled `storageType.fromHex(...)` + `Scale.decode(...)` at a call site. Declare a typed `QueryableStorageEntry` and use `.query()` / `.observe()`.
5. **`major`** — Raw `ByteArray` in a domain model — use `DataByteArray` (see `code/project-types-and-units.md`).
6. **`major`** — Raw `String` for `AccountId` / `EncodedPublicKey` / chain hash — use the typed wrappers.
7. See `code/results-and-errors.md § getOrThrow — forbidden except at two seams` for the canonical rule. (Chain calls returning `Result<T>` must propagate, not unwrap.)
8. **`major`** — Manual `Result<>` for a runtime API that returns `Result<T, E>` — use `ScaleResult.toResult()`.
9. **`major`** — Polling chain state instead of subscribing (`observe`) when the data drives UI.
10. **`major`** — Diffing subscription emissions before persisting — overwrite is fine when updates are rare.
11. **`major`** — Hand-rolling `runCatching` at storage call sites when `queryCatching` / `subscribeCatching` covers it.
12. **`minor`** — Hardcoded base URL when `NetworkApiCreator` provides one.

---

## ChainRegistry — entry point

`chains/src/.../multiNetwork/ChainRegistry.kt` is the singleton root. Inject it where you need a runtime, socket, or chain metadata.

```kotlin
val chain      = chainRegistry.getChain(chainId)
val runtime    = chainRegistry.getRuntime(chainId)        // suspends until FULL_SYNC
val socket     = chainRegistry.getSocket(chainId)         // requires ≥ LIGHT_SYNC
val connection = chainRegistry.getConnection(chainId)
```

Named shortcuts: `chainRegistry.peopleChain()`, `chainRegistry.assetHub()`, etc.

A chain has three possible states: `DISABLED`, `LIGHT_SYNC` (socket only), `FULL_SYNC` (socket + runtime metadata). Code that needs runtime metadata must accept the suspending wait.

---

## Storage reads

Storage entries are typed via `QueryableStorageEntry0..3` in `chains/.../storage/source/query/api/`. Each entry is declared as an extension property on a typed runtime-API receiver (e.g. `SystemRuntimeApi`) using one of the reified `storageN<…>(name)` factory functions.

### Declaring an entry — canonical form

**Use the reified `storageN<T>(name)` form** — it auto-derives the `Auto` encoder backed by `Scale.encode/decode` from the value type. This requires the value (and key) types to be `@Serializable`. This is the recommended path for **all new code**.

```kotlin
// ✓ Canonical — value is @Serializable, encoder derived automatically
context(WithRuntime)
val SystemRuntimeApi.account: QueryableStorageEntry1<AccountId, AccountInfo>
    get() = storage1("Account")

context(WithRuntime)
val MyRuntimeApi.config: QueryableStorageEntry0<MyConfig>
    get() = storage0("Config")
```

### The legacy `binding = ::bindXxx` form

The older overload `storage0(name, binding = ::bindXxx)` uses the `Manual` encoder with a custom binding function over the dynamic instance. **It is legacy.** Existing call sites (`SystemRuntimeApi.number`, `TimestampRuntimeApi.now`) predate the kotlinx-serialization SCALE path; new code should not add more.

```kotlin
// ✗ Legacy. Don't add new entries like this.
val SystemRuntimeApi.number: QueryableStorageEntry0<BlockNumber>
    get() = storage0("Number", binding = ::bindBlockNumber)
```

**When is the legacy form still acceptable?** Only when the value genuinely cannot be modeled as `@Serializable` — e.g. a model that wraps a non-serializable third-party type. In practice this is rare; the right move is to make the value `@Serializable` (it's a domain model under your control) and switch to the reified form.

### Calling it

```kotlin
// One-shot
val info = runtime.metadata.system.account.query(accountId)    // suspend, AccountInfo?
val all  = runtime.metadata.system.account.entries()            // Map<AccountId, AccountInfo>

// Subscription
val flow = runtime.metadata.system.account.observe(accountId)   // Flow<AccountInfo?>
```

Subscription is flow-based; emits current value first, then on every block that modifies the entry. Use `.distinctUntilChanged()` if upstream may emit duplicates.

For multi-key batched subscriptions:
```kotlin
storage.observe(listOf(accountId1, accountId2))  // Flow<Map<AccountId, AccountInfo?>>
```

### Picking one-shot vs subscribe

| You need | Use |
|---|---|
| A snapshot for one operation (validate, compute, submit) | `.query()` |
| Continuously up-to-date data in UI | `.observe()` |
| Historical value at a specific block | `.query(at = blockHash)` |

**Always subscribe over polling** when the data drives UI.

---

## Runtime API calls

`chains/.../call/RuntimeCallsApi.kt`:

```kotlin
suspend inline fun <reified T> RuntimeCallsApi.call(
    section: String,
    method: String,
    arguments: EncodedArguments,
): T
```

Used for read-only chain functions defined in runtime metadata. Example:

```kotlin
val feeResponse: FeeResponse = runtimeCallsApi.forChain(chainId).call(
    section = "TransactionPaymentApi",
    method = "query_info",
    arguments = autoEncodedArgs(
        "uxt" to extrinsic.bytesWithoutLength,
        "len" to extrinsic.extrinsicHex.hexBytesSize().toBigInteger()
    ),
)
```

`autoEncodedArgs` SCALE-encodes each arg via kotlinx-serialization. Return type is decoded automatically via `Scale.decode<T>`. The argument types must be `@Serializable`.

When the runtime function returns a `Result<T, E>`-style enum, decode as `ScaleResult<T, E>`:

```kotlin
sealed class ScaleResult<out T, out E> {
    class Ok<T>(val value: T) : ScaleResult<T, Nothing>()
    class Error<E>(val error: E) : ScaleResult<Nothing, E>()
}

// Convert to Kotlin Result:
val result: Result<Foo> = scaleResult.toResult()
```

---

## SCALE codec — two flavors

### A) Kotlinx-serialization SCALE (BinaryScale)

For types known at compile time. `@Serializable` data classes encoded via the SCALE format. **This is the default — use it for any new binary type you introduce.**

```kotlin
@Serializable
data class AccountData(
    val free: Balance,
    val reserved: Balance,
    val frozen: Balance,
    val flags: AccountDataFlags,
)
```

Special markers:
- `@TransientStruct` — struct with no length prefix (matches SCALE-native struct layout).
- `BigIntegerSerializable` / `ByteArraySerializable` — wrappers for non-native types.

Encode/decode:
```kotlin
val encoded: ByteArray = value.scaleEncodeSerializable()
val decoded: AccountData = Scale.decode(dynamicInstance)
```

For non-standard encodings, define a custom `KSerializer` that goes through `ScaleEncoder`/`ScaleDecoder` (see memory `feedback_kotlinx_scale_serializer`).

### B) Dynamic SCALE (runtime metadata)

For types defined only at runtime by chain metadata. The dynamic instance parsing happens **inside** the storage layer — you almost never write `fromHex` / `Scale.decode` by hand at a call site.

```kotlin
// Internals — what happens behind the reified storageN<T>(name) call:
val dynamicInstance: Any? = storageType.fromHex(runtime, hexValue)
val typed: AccountInfo = Scale.decode(valueType, dynamicInstance)
```

**At call sites you don't supply a binding.** Declare the entry with `storage0<T>(name)` / `storage1<I, T>(name)` etc. The `Auto` encoder handles `Scale.decode` against the reified type. Manual binding (`storage0(name, binding = ::bindXxx)`) is the legacy path — see § "Storage reads" above.

**You don't need to think about dynamic SCALE explicitly for fixed types you control** — just make the model `@Serializable` and use the reified factories.

### Don't hand-roll binary encoders

**Blocking:** Manual binary encoding is forbidden when `BinaryScale` works. New code should never produce a custom binary serializer that bypasses kotlinx-serialization unless there's a documented format incompatibility.

### `DataByteArray` for binary data in domain models

Raw `ByteArray` has reference-equality `equals`/`hashCode`. Use `DataByteArray` (in `common/.../domain/model/`) — it wraps `ByteArraySerializable`, provides content-equality, and renders as hex in `toString()`.

```kotlin
// ✓
data class Proof(val bytes: DataByteArray)

// ✗
data class Proof(val bytes: ByteArray)  // breaks equality, hash, serialization
```

---

## SCALE conformance tests

When you add or change a SCALE-serialized type that lives **on disk** (chat messages, persisted state, push notifications):

1. Add the new shape to `printEncodedValues()` (test utility per memory `feedback_scale_conformance_test`).
2. Run it; copy the produced hex.
3. Write a test that decodes that hex into the latest type.
4. **Never** edit existing hex strings in conformance tests — that's the canary for accidental schema breakage. If you must change the schema in a non-additive way, you also need a migration (see `code/database-and-scale.md`).

---

## Error model

Chain RPC calls throw `RpcException` on network failure. Codec calls throw on type mismatch. Wrap in `Result` at the data layer.

**Use `queryCatching` / `subscribeCatching` for the common path** — they're `StorageDataSource` extensions in `chains/.../storage/source/StorageDataSource.kt` that wrap a `query { … }` / `subscribe { … }` block into a `Result<T>` / `Flow<Result<T>>`:

```kotlin
// ✓ One-shot — gives you Result<AccountInfo?>
val result: Result<AccountInfo?> = storageDataSource.queryCatching(chainId) {
    runtime.metadata.system.account.query(accountId)
}

// ✓ Subscription — gives you Flow<Result<AccountInfo?>>
val flow: Flow<Result<AccountInfo?>> = storageDataSource.subscribeCatching(chainId) {
    runtime.metadata.system.account.observe(accountId)
}
```

`subscribeCatching` integrates with `.withLoading("Tag")` cleanly in a VM since it already produces `Flow<Result<T>>`.

Don't hand-roll `runCatching { ... }` at storage call sites when `queryCatching`/`subscribeCatching` covers it. (Avoid `runCatching { throw ... }` patterns — see `code/results-and-errors.md`.)

For runtime API calls that return `Result<T, E>` on the chain side, decode as `ScaleResult` and convert via `.toResult()` so the failure becomes a regular Kotlin `Result.failure(ScaleResultError(error))`.

---

## Subscriptions — connection state

Subscriptions live as long as the chain socket is connected. When the connection drops:
- The flow does not error by default; it stalls until reconnect.
- For background work, the default connection isn't always active — request one via `ChainConnectionRefCounter` (see `transactions.md § Background chain work`).
- Don't depend on a specific node URL; collation may transparently switch. When you must hit a specific node (e.g. HOP file fetch), open a dedicated socket — don't rely on the chain connection.

---

## Where new things live

| Concept | Goes in |
|---|---|
| Storage entry wrapper | `chains/.../storage/source/query/api/<Pallet>Storage.kt` |
| Runtime API wrapper | `chains/.../call/api/<Pallet>RuntimeApi.kt` |
| Domain model decoded from chain | `feature/<X>/api/.../domain/model/` (with `@Serializable` if encoded over the wire) |
| Custom `KSerializer` for an exotic encoding | `feature/<X>/impl/.../data/scale/` or `common/utils/scale/` if shared |
| SCALE conformance test | `feature/<X>/impl/src/test/...ScaleConformanceTest.kt` |

---

## Anti-patterns

- **Polling** chain state instead of subscribing — chain pushes events; use `observe`.
- **Manual binary encoders** when `BinaryScale` covers the case.
- **`storage0(name, binding = ::bindXxx)`** for a new entry — legacy. Use the reified `storage0<T>(name)` form; make the value `@Serializable` if it isn't already.
- **Hand-rolled `fromHex(...)` + `Scale.decode(...)`** at a call site instead of declaring a typed `QueryableStorageEntry`.
- **Diffing** subscription emissions before persisting — overwrite is fine if updates are rare.
- **Raw `ByteArray`** in data classes — use `DataByteArray`.
- **`String`** for `AccountId` / `EncodedPublicKey` / hashes — use the typed wrappers.
- **`getOrThrow()`** on a chain call — see `code/results-and-errors.md § getOrThrow`.
- **Hand-rolled `Result<>`** for runtime-API-returning-Result calls — use `ScaleResult.toResult()`.
