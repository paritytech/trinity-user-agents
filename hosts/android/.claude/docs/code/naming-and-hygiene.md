# Naming and Code Hygiene

How we name things, comment, structure files, and keep noise out.

## Rules at a glance

1. **`major`** — `Service` suffix is reserved for Android `Service` subclasses. Use `Manager` / `Coordinator` / `Engine` / `Pipeline` otherwise.
2. **`major`** — Don't create a `UseCase` that's a single-line passthrough. Inline at the caller.
3. **`major`** — No default values in data-carrying constructors (requests, payloads, metadata, domain models).
4. **`major`** — Don't expose mutable class fields for testability — constructor-inject the collaborator.
5. **`major`** — Timber-only logging. No `android.util.Log.x(...)`. Match log level to severity; don't log expected branches at `error`.
6. **`major`** — No PII in logs (mnemonics, signatures, user-typed content). Account IDs / public keys / addresses are **not** PII for this project and may be logged.
7. **`major`** — Magic numbers extracted to named constants in a companion.
8. **`major`** — `Duration` / `Instant` over raw `Long` for time. See `code/project-types-and-units.md`.
9. **`major`** — Imports always; never write fully-qualified types inline.
10. **`major`** — JSON encoding / decoding goes through kotlinx-serialization (`@Serializable` + `Json.encodeToString` / `Json.decodeFromString`). No hand-rolled `buildJsonObject { … }` walks, no Gson, no manual `JSONObject`. Custom wire shapes use a `KSerializer`. Same family as rule "Manual binary encoder where `BinaryScale` covers it" in `code/database-and-scale.md § Rules at a glance`.
11. **`major`** — Minimal comments — mandatory. Default to none; write one ONLY where the code is genuinely specific and a competent reader could misread the logic without it (non-obvious invariant, workaround, platform quirk). Never restate what the code does.
12. **`minor`** — KDoc only on public `api/` interfaces and their non-trivial methods; not on `impl/` classes.
13. **`minor`** — Method/class name must reflect what it does now; rename when behavior changes.
14. **`minor`** — Trailing positional `Boolean` parameter — use named-call site or split into two methods.
15. **`minor`** — Package leaves are `camelCase` (`pairRequest`), never lowercase-glued (`pairrequest`).
16. **`minor`** — Privates at the bottom of the class (Kotlin convention).
17. **`minor`** — Don't class-level a variable used in one method.
18. **`minor`** — Group exploding parameter lists into nested data classes.
19. **`minor`** — Compose / Kotlin file > ~400-500 lines should split.
20. **`minor`** (PR hygiene) — One task per PR; attach demo video for visible features.
21. **`minor`** — Enums representing ordered / comparable categories must be backed by a numeric field, e.g. `enum class SlotPriority(val level: Int) { Normal(0), High(1) }`. **All comparisons, filters, sorts, and SQL orderings go through that numeric field — never through enum identity or name.** Adding a new tier is then a one-line enum entry; no `when` / `if` branch in filter or sort logic needs to change.

---

## Comments — only "why", never "what"

Minimal comments are **mandatory**, not a preference. Default to **no comments** — the code must explain itself through naming. Write a comment ONLY where the code is genuinely specific and a competent reader could misread the logic without it. A comment is justified only when:

1. **A non-obvious invariant or workaround** — "this must run before X because the runtime expects the call wrapped" / "ignoring this branch because the backend always returns it but we don't need it".
2. **A subtle algorithm** — geometric drawing math, custom encoding, multi-step state machine.
3. **A documented interface contract** (see § KDoc below).
4. **A TODO that names what's missing and a condition or owner** — "TODO: support reactions on attachments; needs RFC".

Never write:
- "this is a confetti burst" (the function is `drawConfettiBurst`, the comment carries no information).
- A summary header restating the class signature.
- Commit-message-style explanation of "what I changed".

## KDoc — where it's worth writing

KDoc lives on:

- **Public `api/` interfaces** and their non-trivial methods. The contract is consumed by other modules without seeing the implementation, so the *what + when + invariants* must be stated.
- **Sealed types that encode business meaning** — what each variant represents.
- **Methods with non-trivial suspension or threading semantics** — "Suspends until FULL_SYNC; safe to call from any context".

KDoc does **not** belong on:
- `impl/` classes (the code is the contract).
- Repository methods that mirror an interface.
- Self-explanatory functions (`fun isLoading(): Boolean`).

```kotlin
// ✓ KDoc on public api
/**
 * Resolves a [ProductId] from a `.dot` URL. Returns null when the URL doesn't
 * resolve to a known `.dot` domain.
 *
 * Safe to call before chain sync.
 */
fun resolveProduct(url: Uri): ProductId?

// ✗ Useless
/** Gets the user's name. */
fun getName(): String
```

---

## Naming

### Classes

| Pattern | When |
|---|---|
| `Real<InterfaceName>` | Hilt-bound impl of a public interface (e.g. `RealAmountInputMixin`). |
| `<Feature>...` prefix | Cross-feature shared things in `common`/`database`/`design` need the feature name to disambiguate (e.g. database entities — `CoinageNoteEntity`, not bare `NoteEntity`). |
| `<Action>UseCase` | Reusable cross-feature business logic. |
| `<Screen>Interactor` | Per-screen orchestration. |
| `<Screen>ViewModel` / `<Screen>Contract` | Presentation. |
| `<Type>State`, `<Type>UiState` | State data classes. |

### Methods

Names describe **what the function does in the present**. Renaming when behavior changes is mandatory.

- ✓ `cancelGameAboutToStartNotification()` — describes what it cancels.
- ✗ `handleEvent()`, `performAction()` — generic and uninformative.
- ✗ Names that drift from behavior — `getNotUsedCounterIndices` that started returning a default — rename or restore.

### `Service` suffix

Reserved for **Android `Service`** subclasses only. Don't suffix any other class `Service`. Plain classes use `Manager`, `Coordinator`, `Engine`, `Pipeline`, etc. as appropriate.

### Boolean parameters

Avoid trailing positional `Boolean` flags. Either name them at the call site, or replace with an enum/sealed type when there are 2+.

```kotlin
// ✗
FileOutputStream(file, true)

// ✓
val append = startFrom > 0
FileOutputStream(file, append)

// ✓✓ when policy varies
allocator.allocate(slot, policy = OnExistingSlotPolicy.AllocateAdditional)
```

### Package leaves: camelCase

See `CLAUDE.md` rule 12. `presentation/pairRequest/`, not `presentation/pairrequest/`.

### Imports

See `CLAUDE.md` rule 14. Always import; never inline fully-qualified types. Reviewer flags as minor; Detekt should enforce.

---

## Defaults in data classes

**No default values** in data-carrying constructors (requests, payloads, metadata, domain models). Defaults hide what the caller is choosing.

```kotlin
// ✗
data class HandshakeOffer(val url: String, val timeout: Duration = 30.seconds, val retries: Int = 3)

// ✓
data class HandshakeOffer(val url: String, val timeout: Duration, val retries: Int)
```

Defaults are acceptable on:
- UI state placeholders only in `@Preview` builders, not in the data class itself.
- Configuration where genuinely-optional collaborators are nullable and the default is `null`.

---

## Visibility & scoping

### Privates at the bottom of the file

Kotlin convention. Public/Contract methods on top of the class, private helpers at the bottom.

### Don't class-level a local variable

```kotlin
// ✗
private var lastPlayedSugar = 0
private fun observeSugarLevelForSound() {
    if (lastPlayedSugar != current) { ... }
}

// ✓ — only used inside one method
private fun observeSugarLevelForSound() {
    var lastPlayed = 0
    ...
}
```

### Don't expose mutable fields for testability

Inject collaborators via constructor, don't expose `var audioPlayerFactory: AudioPlayerFactory` so a test can swap it.

---

## Time and units

Prefer typed time/duration over raw `Long`:

```kotlin
// ✓
val timeout: Duration = 5.seconds
val at: Instant = Instant.now()

// ✗
val timeoutMillis: Long = 5_000
val atUnix: Long = ...
```

(`kotlinx.datetime` `Instant`; `kotlin.time.Duration`.)

---

## Grouping exploding parameter lists

When a class/function has many parameters that belong in sub-groups, introduce a nested data class:

```kotlin
// ✗
class UploadJob(
    val uploadedChunks: Int,
    val error: Throwable?,
    val status: UploadStatus,
    val mimeType: String,
    val originalFileSize: Long,
    val totalChunks: Int,
)

// ✓
class UploadJob(
    val uploadState: UploadState,    // uploadedChunks, error, status
    val meta: UploadMeta,            // mimeType, originalFileSize, totalChunks
)
```

---

## Logging policy

### Timber only

Never use `android.util.Log.x(...)` directly. Always Timber.

```kotlin
Timber.e(throwable, "Failed to fetch X")
Timber.w("Got expected fallback for Y")
Timber.i("Game session started")
Timber.d("Encoded payload bytes: %s", bytes.toHex())
```

### Level discipline

| Level | When |
|---|---|
| `error` | Unexpected, actionable. A bug or a system failure we'd want to surface in monitoring. |
| `warn` | Expected failure path the user might see (network down, validation rejection, fallback used). |
| `info` | Major lifecycle / state transitions worth seeing in normal logs (service started, login, game phase change). |
| `debug` | Developer diagnostics. Stripped from release builds. |

✗ Logging an **expected** branch at `error` level.

### Don't log noise

- ✗ Per-frame logs in a tight loop.
- ✗ "Entering method X" / "Exiting method X" trace logs.
- ✗ Echoing data that's already visible in the next log line.
- ✗ Stack traces at `info` / `debug`.

### No PII

Never log:
- Mnemonics, signatures, private payload bytes.
- User-typed content (chat text, search queries).

Account IDs / public keys / addresses are **not** PII for this project — they are public on-chain identifiers and are fair game for logs. Use them freely to trace flows.

---

## Magic numbers and constants

Extract magic numbers into named constants. Group related ones in a companion object.

```kotlin
// ✗
delay(7_500L)
val maxRetries = 3
if (jitter < 0.15f) ...

// ✓
companion object {
    private val HANDSHAKE_TIMEOUT = 7.5.seconds
    private const val MAX_RETRIES = 3
    private const val JITTER_THRESHOLD = 0.15f
}
delay(HANDSHAKE_TIMEOUT)
```

---

## File splitting threshold

Split a file when:
- A Compose file passes ~400 lines (see `ui-compose.md`).
- A Kotlin class file passes ~500 lines.
- Two distinct concerns share a file because they grew together (split + cross-link).

The split goal isn't line-count purity — it's "can a new reader find what they're looking for".

---

## Test discipline

Reviewer flags missing unit tests only when:
- The change adds **non-trivial domain/data logic** — mappers, calculators, state machines, codecs, parsers.
- The file already has a sibling `*Test.kt` (regression coverage expected).
- SCALE encoding changed (conformance test mandatory — see `database-and-scale.md`).

UI tests and VM tests are not required by default.

---

## PR hygiene (reviewer enforces as minor)

- **One task per PR.** Multiple unrelated changes → split.
- **Attach a demo video** for visible features / UI changes.
- **No mixed refactors** in a feature PR unless explicitly called out in the description.
