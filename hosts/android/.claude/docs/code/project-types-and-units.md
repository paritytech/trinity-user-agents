# Project Types and Units

The codebase has a small set of canonical wrapper types that every feature should use instead of bare primitives. Reaching for `Long`, `ByteArray`, or `String` when one of these exists is a code-quality regression — it loses unit safety, content-equality, or invariant enforcement.

## Rules at a glance

1. **`major`** — `kotlin.time.Duration` for time spans; `kotlinx.datetime.Instant` for moments. Don't expose `timeMillis: Long` / `durationSeconds: Int`.
2. **`major`** — `InformationSize` for file/buffer/upload sizes. Don't expose `maxSizeBytes: Long`.
3. **`major`** — `DataByteArray` for binary blobs in data classes. Raw `ByteArray` has reference-equality `equals`; the hook flags this.
4. **`major`** — Typed `AccountId` / `EncodedPublicKey` / chain-hash wrappers — not `String`.
5. **`major`** — Domain identifiers with invariants use `@JvmInline value class` with private constructor + factory functions. New domain types without invariants use `typealias`. Primitives only for trivial values.
6. **`minor`** — `Balance` / `BlockNumber` / `Nonce` substrate primitives over raw `BigInteger` / `Long`.

---

## Time and durations — `Duration` and `Instant` from `kotlin.time` / `kotlinx.datetime`

Don't expose time as `timeMillis: Long`, `durationSeconds: Int`, or any other unit-suffixed primitive. Use:

- **`kotlin.time.Duration`** for spans (`5.seconds`, `2.hours`, `300.milliseconds`).
- **`kotlinx.datetime.Instant`** for points in time (event timestamps, expiry, "now").

```kotlin
// ✗
data class HandshakeOffer(
    val createdAtMillis: Long,
    val timeoutSeconds: Int,
)

// ✓
data class HandshakeOffer(
    val createdAt: Instant,
    val timeout: Duration,
)
```

Benefits:
- No "is it seconds or millis?" confusion at any call site.
- Arithmetic is dimensionally checked: `Instant + Duration` is an `Instant`; you can't accidentally add a millisecond count to a microsecond count.
- Display formatting uses the same primitives across the app.

### When `Long` is still right

- Block numbers (`BlockNumber` — semantically a count, not time).
- Nonces, counters.
- Anything where the unit is "count of items" rather than time.

---

## `InformationSize` — file/buffer/upload sizes

`common/src/.../utils/InformationSize.kt` wraps a byte count in a value class with named unit constructors and an arithmetic API:

```kotlin
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.megabytes
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.kilobytes
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes

val limit: InformationSize = 25.megabytes
val chunk: InformationSize = 64.kilobytes
val remaining: InformationSize = limit - already

// Reading components for display:
val pretty: String = size.toString()       // "1.2MB" style via toComponents()
val rawBytes: Long = size.inWholeBytes
```

Use `InformationSize` for: upload/download limits, chunk sizes, file sizes, in-memory buffer thresholds, network quotas.

```kotlin
// ✗
fun upload(file: File, maxSizeBytes: Long): Result<Unit>

// ✓
fun upload(file: File, maxSize: InformationSize): Result<Unit>
```

---

## `DataByteArray` — binary blobs in domain models

Raw `ByteArray` has reference-equality `equals` / `hashCode`, so any data class carrying a `ByteArray` field silently breaks equality and de-duplication.

`DataByteArray` (in `common/.../domain/model/`) wraps `ByteArraySerializable` with content-equality, content-hash, and a hex `toString()`.

```kotlin
// ✗
data class Proof(val bytes: ByteArray)

// ✓
data class Proof(val bytes: DataByteArray)
```

Memory: `feedback_data_byte_array`.

---

## Typed identifiers — `AccountId`, `ChainId`, `ProductId`, ...

See `architecture/maintainability.md § Narrow typing`. Quick reminder:

- `@JvmInline value class` when an invariant must hold (private constructor + `fromXxx` factories).
- `typealias` when a new domain type adds readability without invariant.
- Primitive only when truly trivial.

Don't pass `String` for `AccountId` / `EncodedPublicKey` / chain-hash — use the typed wrappers.

---

## `Balance`, `BlockNumber`, `Nonce` — substrate-side primitives

These are typealiased in `chains/` for the substrate types. Use them rather than `BigInteger` / `Long` so domain code reads as substrate-native:

```kotlin
// ✓
fun transfer(amount: Balance, fee: Balance): Result<TxHash>
```

---

When you find a function signature that takes a bare primitive for one of these concepts, **change it** — that's the cheapest type-safety improvement available and the convention everywhere else in the codebase.
