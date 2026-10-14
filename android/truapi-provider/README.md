# TrUAPI Android chain transport

*Kotlin bindings for the `truapi-provider` crate (UniFFI). An embedded smoldot light client and the bundled chain-spec catalog stay in Rust; the host addresses a chain by genesis hash and exchanges JSON-RPC strings.*

> **Status:** there is no remote coordinate yet. JitPack cannot serve this module as it stands — it builds from a git tag, and both the bindings and the cdylib are generated rather than committed — and no hosted Maven publication is wired up. Until one is, integrate with `make provider-android-publish-local` + `mavenLocal()`. The module itself has not been built on CI or a machine with the Android toolchain, so treat the Gradle wiring below as unverified.

Unlike [`truapi-host`](../truapi-host), whose AAR leaves the cdylib to the integrator, this AAR bundles `libtruapi_provider.so` for every published ABI. That is the point of the package: a consumer adds one coordinate and calls `ChainProvider()`, with no Rust toolchain and no dependency on the crate.

## Consume

```kotlin
// settings.gradle.kts
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
        mavenLocal() // until a hosted publication exists, see Status above
    }
}
```

```kotlin
// app/build.gradle.kts
dependencies {
    implementation("io.parity:truapi-provider-android:0.1.0")
}
```

The consuming app must declare `android.permission.INTERNET` — the light client dials peers over TCP and WebSocket.

Chain specs are compiled into the cdylib, so the app ships no spec files of its own and never refreshes them. Picking up a spec refresh means taking a newer version of this artifact.

### Compatibility

- **minSdk**: 29 (Android 10). Matches the `truapi-host` floor so a host can depend on both.
- **ABIs**: `arm64-v8a`, `armeabi-v7a`, `x86_64` (`ANDROID_ABIS` overrides the set). Each carries a copy of the light client, so the AAR is large; split APKs or an App Bundle keep the shipped size to one ABI.
- **Transitive dependency**: the AAR pulls `net.java.dev.jna:jna:5.14.0` (UniFFI's runtime), shared with `truapi-host` when both are present.

## Public surface

Regenerate the bindings and the `.so` together. The generated Kotlin declares a checksum guard but never invokes it, so a stale `.so` paired with fresh bindings is not detected on this platform, and a callback whose arity changed keeps the same C symbol name, so the linker does not catch it either. iOS runs the equivalent guard before installing the callback vtable.

Everything is generated from [`ffi.rs`](../../rust/crates/truapi-provider/src/ffi.rs) into `uniffi.truapi_provider.*`:

- `ChainProvider` - construct **one per process** and share it. Every connection runs on the single embedded light client, so they share sync, peers, and warm state while keeping their own request queue and response stream. `connect(genesisHash, listener)` resolves the network from the bundled catalog (relay wiring and statement-store placement included), so the 32-byte genesis hash is the only argument. At most 32 connections are held at once: past that, `connect(genesisHash, listener)` throws `ChainProviderException.Connect` rather than adding another chain, and `disconnect()` hands the slot back. That ceiling is a backstop against connections you never close, not a budget to spend; the catalog resolves eight chains, so one reused connection per chain stays well under it.
- `ChainMessageListener` - the host implements it; `onMessage(message)` receives each JSON-RPC response and notification, `onClosed(reason)` fires once the pump stops and names why. Both may throw: a listener that throws stops the pump for that connection rather than being called again for every response, and an exception it does not declare is reported as `ChainProviderException.Listener` instead of aborting the process.
- `ChainCloseReason` - `StreamEnded` when the response stream ended, which includes your own `disconnect()` coming back to you, and `ListenerFailed` when your listener rejected a message and the connection was closed for it. It says why the pump stopped, not whether you should reconnect; keep an `else` branch, since variants may be added. That is source compatibility only: adding a variant does not change the `onClosed` checksum, so bindings older than the `.so` pass the integrity check and then fail to decode the reason, which surfaces as `onClosed` never firing. `reason` on `ListenerFailed` is bounded to 256 Unicode scalar values, which is up to 512 in Kotlin's `String.length` (UTF-16 code units) and up to 1024 bytes. Reconnect from a *serial* executor off the pump thread: `connect(genesisHash, listener)` refuses to run inside a listener callback and throws `ChainProviderException.Connect` if you try. Do not re-queue work with `send(request)` from `onClosed`: the connection is already closed by then, and `send` on a closed connection is dropped silently, with no exception and no response frame.
- `ChainConnection` - `send(request)` queues a request, `disconnect()` tears the connection down. It is not called `close`, because uniffi's generated Kotlin object already has `AutoCloseable.close()` for handle disposal.
- `ChainProvider.withStorage(store)` - build a provider that resumes each chain from stored finalized state instead of warp syncing from the checkpoint in the chain spec. The plain `ChainProvider()` stores nothing, so `loadDatabase` and `saveDatabase` throw on it rather than doing nothing. The crate ships no storage of its own: you own where the bytes live, and with it whether they survive a cache eviction.
- `StorageClient` - implement it over your own storage, typically `context.filesDir` rather than `cacheDir`, which the OS reclaims under pressure. `load(genesisHash)` answers the stored string or `null`, `save(genesisHash, blob)` replaces it, and both take a 32-byte hash as `ByteArray`. A client that cannot answer must throw: returning no blob means "nothing stored yet" and lets the next write replace good state. Both are suspending functions awaited on the thread the caller drives, so the implementation must not require `Dispatchers.Main`.
- `loadDatabase(genesisHash)` - read the stored state for a chain in before connecting to it. Call it first: smoldot consumes a blob only on the first add of a chain, so a later call cannot take effect and answers `false`.
- `saveDatabase(genesisHash)` - snapshot the finalized state of a running chain into your storage. Answers `false`, without writing, for a chain that has finalized nothing yet or that the embedded client is not running. It is a full round trip through the light client, so drive it while the app is alive and treat a call from `onPause` as best effort.
- `watchLifecycle(genesisHash, listener)` - watch the sync progress of a chain something is connected to. Your `ChainLifecycleListener` gets `onLifecycle(state)` with the current `ChainLifecycle` (`phase`: `Connecting`, `Syncing(at, target)` or `Ready`; `peers`; `health`: `Ok` or `Stalled(reason)`), then once per change, on a background thread the crate owns. `onEnded()` fires once the watch ends: `stop()` on the returned `LifecycleWatch`, the chain no longer running, or your listener throwing. Closing the watch handle stops it too. A parachain goes from `Connecting` straight to `Ready`, so watch its relay for warp sync progress. Requests sent on a connection are held until the chain first reaches `Ready`, apart from chain-spec queries, statement-store and Bitswap calls.
- `ChainProviderException` - `Connect` when the genesis is outside the catalog or the transport fails, `BadGenesis` when the hash is not 32 bytes, `Listener` when the listener the host installed failed in a way it did not declare, `Lifecycle` when the chain to watch is not running, `Storage` when your storage failed or the provider has none. Your client reports its own failures as `StorageClientException`. Adding a case here does not change the checksum of the methods that return it, so bindings older than the `.so` pass the integrity check and then fail to decode the new case. Regenerate bindings and `.so` together, exactly as `ChainCloseReason` above requires.

## Architecture

```text
host app
  ChainProvider().connect(genesisHash, listener)
           |
           v
libtruapi_provider.so (embedded smoldot + bundled chain-spec catalog)
  → one light client per process, one added chain per connection
  → responses pumped on a Rust-owned thread into ChainMessageListener
```

A connection is a raw JSON-RPC string pipe. The provider does no decoding: what smoldot answers is what the listener receives.

## Example

> **Threading:** the crate pumps each connection's responses on a background thread it owns, so `onMessage` and `onClosed` are never called on the UI thread — marshal any UI work onto it with `Handler(Looper.getMainLooper())` or a `Dispatchers.Main` `CoroutineScope`. `connect(genesisHash, listener)` is blocking and adds the chain on the calling thread, so keep it off the UI thread.

```kt
import android.os.Handler
import android.os.Looper
import uniffi.truapi_provider.ChainCloseReason
import uniffi.truapi_provider.ChainConnection
import uniffi.truapi_provider.ChainMessageListener
import uniffi.truapi_provider.ChainProvider

class Responses : ChainMessageListener {
    private val main = Handler(Looper.getMainLooper())

    // A JSON-RPC response or subscription notification, verbatim from smoldot.
    override fun onMessage(message: String) {
        main.post { /* decode and render */ }
    }

    override fun onClosed(reason: ChainCloseReason) {
        // Reached whichever way the connection ended, including your own
        // disconnect(). Reconnect on your own intent, not on this alone.
        main.post {
            when (reason) {
                is ChainCloseReason.ListenerFailed -> { /* this listener rejected a message: reason.reason */ }
                else -> { /* drop the connection */ }
            }
        }
    }
}

// One provider per process; hold it for the app's lifetime.
val provider = ChainProvider()

// 32 raw bytes, not a hex string. Must be a chain in the bundled catalog.
val genesis = ByteArray(32)
val connection: ChainConnection = provider.connect(genesis, Responses())

connection.send("""{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_genesisHash","params":[]}""")

// On teardown:
connection.disconnect()
```

## Maintainers: publishing locally

```bash
make provider-android-publish-local
```

That regenerates the Kotlin bindings, cross-compiles the cdylib for every ABI, and publishes to `~/.m2/repository/io/parity/truapi-provider-android/<version>/`. It needs Gradle, JDK 17, the Android NDK, and `cargo-ndk` (`cargo install cargo-ndk`).

Publishing refuses to run when `src/main/jniLibs` holds no `libtruapi_provider.so`: such an AAR resolves fine and then fails at the first `ChainProvider()` with `UnsatisfiedLinkError`, which is a much worse failure than a build error. The two steps behind it are available separately:

```bash
make provider-kotlin       # regenerate the bindings only
make provider-android-jni  # cross-compile the cdylib only
```

Both the generated bindings under `src/main/kotlin/generated/uniffi/` and the `.so` files under `src/main/jniLibs/` are gitignored build outputs.

## Regenerating the UniFFI bindings

```bash
make provider-kotlin
```

That builds the crate with the `codegen` profile and runs the workspace `uniffi-bindgen-cli`. The `codegen` profile is required because uniffi-bindgen scans the cdylib's exported metadata symbols, which the `release` profile strips — a plain `--release` build produces a stripped library and no bindings. (`make uniffi-kotlin` does the same for the host package.)
