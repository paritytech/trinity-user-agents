# Database and SCALE

Room is the storage layer; SCALE codec is the wire format. Both are migration-sensitive — getting these wrong corrupts users' data. This doc codifies the safe patterns.

## Rules at a glance

1. **`blocking`** — Editing existing files under `database/schemas/` is forbidden (the `PreToolUse` hook will block). Schema files are append-only; bump the DB version and add a `Migration(N, N+1)` instead.
2. **`blocking`** — Editing existing hex in a SCALE conformance test is forbidden — the test exists to fail loudly when encoding changes. If the schema must change, add a migration and add a new test.
3. **`blocking`** — Persisted SCALE schemas changed non-additively (remove / reorder / retype field, insert enum variant in middle, add struct field) must ship a `*V<N>.kt` snapshot of the old shape plus a migration.
4. **`blocking`** — Room schema bump (`@Database(version = N+1, ...)`) must add a paired `Migration(N, N+1)` and pass `MigrationTest.migrateAll()`.
5. **`major`** — New SCALE type missing a conformance test.
6. **`major`** — Manual binary encoder where `BinaryScale` / `@Serializable` covers it.
7. **`major`** — Repository accessing `Preferences` directly. Use a typed `XxxStorage` interface.
8. **`major`** — Feature-specific entity placed in shared `database/.../entity/` without a feature-prefixed class name (e.g. `CoinageSessionEntity`, not bare `SessionEntity`).
9. **`major`** — DAO returning `List<…>` just to check existence — use `@Query("SELECT EXISTS(...)")`.
10. **`major`** — Legacy `Entry<N>Encoders` introduced in new code (only for migrating old data).
11. **`minor`** — Append-only addition at the **end** of an enum is the one SCALE additive change that doesn't need a migration.

---

## Room — where things live

```
database/
├── src/main/kotlin/io/paritytech/polkadotapp/database/
│   ├── DatabaseModule.kt        ← Hilt provides AppDatabase
│   ├── AppDatabase.kt           ← @Database(entities = [...], version = N)
│   ├── entity/                  ← @Entity classes (cross-feature)
│   ├── dao/                     ← @Dao interfaces
│   ├── migrations/              ← Migration objects + per-version SCALE schemas
│   └── converters/              ← @TypeConverter classes
└── schemas/                     ← Room-generated JSON schemas (track in git)
```

### Where a new entity lives

| Scope | Goes in |
|---|---|
| Shared across multiple features (Account, Chain, ChatMessage) | `database/.../entity/` |
| Feature-private | `feature/<X>/impl/.../data/local/entity/` — kept in the feature module's part of the schema |

If you put a feature-specific entity in shared `database`, **prefix the class name with the feature** (`CoinageSessionEntity`, not `SessionEntity`).

### Repositories own entity ↔ domain mapping

Entities never escape the data layer. Repositories receive entities from DAOs and return domain models. Mapping lives in `feature/<X>/impl/.../data/mappers/` as extension functions.

```kotlin
// ✓
class RealFooRepository @Inject constructor(private val dao: FooDao) : FooRepository {
    override fun observe(): Flow<List<Foo>> = dao.observe().map { rows -> rows.map { it.toDomain() } }
}

// ✗ — Room types in domain
override fun observe(): Flow<List<FooEntity>>
```

### Typed `XxxStorage` interfaces, not raw `Preferences`

For key-value style state (flags, last-seen IDs, single-tenant prefs), define a typed interface, not direct `Preferences` access from repositories:

```kotlin
// ✓
interface CredentialClaimedStorage {
    suspend fun isClaimed(accountId: AccountId): Boolean
    suspend fun setClaimed(accountId: AccountId)
}

class RealCredentialClaimedStorage @Inject constructor(
    private val preferences: Preferences,
) : CredentialClaimedStorage { ... }
```

---

## Room migrations

When you change the schema:

1. **Bump the version** in `@Database(version = N+1, ...)`.
2. **Add a `Migration(N, N+1)`** object to `AppDatabase.migrations`.
3. **Run `MigrationTest.migrateAll()`** (`database/src/androidTest/.../migrations/MigrationTest.kt`) — it builds the DB at v1 then walks every registered migration through to the latest version, exercising structural correctness end-to-end. This is the test you depend on; specific per-pair tests (`Migration24To25Test`) layer data-content assertions on top.
4. **Don't forget feature-private entities** living in feature modules — they participate in the same DB.

### Room schema files are append-only

The JSON files under `database/schemas/` are Room's record of each schema version. **A PR must never modify an existing schema file** — they only ever add a new file for the new DB version. If your change appears to require editing `database/schemas/<n>.json`, you're really making a non-additive change to schema version `n` instead of authoring `n+1`. Bump the version and add a new file.

Reviewer: **blocking** when an existing schema file under `database/schemas/` is modified by the diff.

### SQL queries should be efficient — push aggregates to SQL

```kotlin
// ✗ — load all blocked contacts to check existence
suspend fun hasBlockedContacts(): Boolean = dao.getAllBlocked().isNotEmpty()

// ✓
@Query("SELECT EXISTS(SELECT 1 FROM contacts WHERE blocked = 1)")
suspend fun hasBlockedContacts(): Boolean
```

---

## SCALE — when migrations are required

SCALE encoding is **positional**, not field-name-keyed. Reordering, retyping, or removing a field silently corrupts data on disk for any user who upgrades. This is why we treat SCALE-encoded persisted types as carefully as DB schemas.

### When you MUST add a SCALE migration

For any SCALE-serialized type that lives **on disk** (chat message content, push-notification payloads, persisted SCALE schemas):

| Change | Migration required? |
|---|---|
| **Remove a field** | Yes |
| **Reorder fields** | Yes |
| **Retype a field** (e.g. `u32` → `u64`) | Yes |
| **Rename a variant in a sealed/enum** (positional) | No (name doesn't affect encoding) |
| **Insert a new variant in the middle of an enum** | Yes |
| **Append a new variant at the end of an enum** | **No** — pure-append is the one safe additive change (existing variants encode identically; old code falls through on unknown variants) |
| **Add a new field to a struct** | Yes — SCALE has no skip-unknown |

### How to add a SCALE migration

The recipe:

1. **Copy the pre-change type to a versioned file**: `ChatMessageContentLocalV24.kt` containing the exact pre-PR shape.
2. **Update the live type** with the new shape.
3. **Write a migration** that reads `V24` bytes and produces the new shape. Be **extremely careful** with indices and order — when asking an AI to write it from scratch, double-check the index mapping against the V24 file.
4. **Add a conformance test** for the new shape (see below).

Editing a SCALE-encoded persisted type in place is OK **only when accompanied by a migration**. A diff that touches the live shape **must** also add the `*V<N>.kt` snapshot and the migration; otherwise older nightly users' on-disk bytes are corrupt on upgrade. Reviewer: **blocking** when a persisted SCALE type changes shape without a paired migration + conformance test.

### SCALE conformance tests — the canary

Conformance tests exist precisely to **fail loudly** when a change accidentally breaks SCALE encoding.

The flow:
1. Add the new value to `printEncodedValues()` (a test utility).
2. Run it; copy the produced hex output.
3. Add a test that decodes that hex into the latest type.

**Never edit existing hex strings in conformance tests.** If you find yourself doing it, that's the signal that you've changed encoding non-additively — add a migration.

Tests live in `feature/<X>/impl/src/test/.../ScaleConformanceTest.kt` (or in the database module for shared schemas).

---

## SCALE encoding — patterns

Full details in `architecture/chain-integration.md § SCALE codec`. Quick code-level reference:

### `@Serializable` with kotlinx-serialization SCALE

```kotlin
@Serializable
data class ChatMessageContentV24(
    val sender: AccountIdSerializable,
    val payload: ByteArraySerializable,
    val timestamp: BigIntegerSerializable,
)
```

- `@TransientStruct` for length-prefix-less structs.
- `ByteArraySerializable`, `BigIntegerSerializable`, `AccountIdSerializable` wrap non-native types.
- `DataByteArray` for in-memory domain models that carry raw bytes — proper `equals` / `hashCode`.

### Custom KSerializer

For exotic encodings, write a `KSerializer` that goes through `ScaleEncoder` / `ScaleDecoder` directly. Pattern in memory `feedback_kotlinx_scale_serializer`. Use a `@JvmInline value class` wrapping the raw type, with the custom serializer attached.

### Don't hand-roll binary encoders

If you find yourself manually packing bytes, switch to `BinaryScale`. Hand-rolled binary code rarely matches the canonical encoding under all edge cases (length prefixes, var-ints, etc.).

---

## Legacy `Entry<N>Encoders` and other backward-compat layers

The codebase has legacy hooks like `Entry3Encoders` (in `chains/.../storage/source/query/api/QueryableStorageEntry3.kt`) that exist solely to support entries with custom manual binders for backward compatibility with older code that predates kotlinx-serialization SCALE. **New entries should not reach for these** — declare the value type as `@Serializable` and use the reified `storageN<T>(name)` factory (see `architecture/chain-integration.md § Storage reads`).

Don't proactively introduce manual `Entry<N>Encoders` wrappers around new code; the `Auto` encoder via `Scale.encode/decode` is the canonical path.

---

## Push-notification payloads — also SCALE

Notification payloads (and similar wire-style payloads) follow the same rules: any change to the on-wire shape is a migration, conformance tests cover it.

The push handler is allowed to span all three layers (data / domain / presentation) currently — it's an acknowledged seam that will be properly layered later. When adding to it, keep the additions in obvious places (data: payload parsing; domain: state computation; presentation: notification rendering).

| Tempted to write a manual binary encoder | Don't. Use `BinaryScale`. |
