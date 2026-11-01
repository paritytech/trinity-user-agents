package io.parity.truapi

import java.util.WeakHashMap
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.truapi.HostRejection
import uniffi.truapi.nativeDescribeCoreStorageKey

/** One coordinator for every adapter in this process, including independent Rust roots. */
internal object CoreStorageAccess {
    // Collisions only serialize unrelated slots. The bounded table never forgets a held slot lock.
    private val locks = Array(256) { Mutex() }
    private val runtimes = WeakHashMap<TrUAPIHostRuntime, String>()
    private val failures = WeakHashMap<Any, Boolean>()
    private sealed interface Notice {
        class Changed(
            val storage: HostCoreStorage,
            val origin: Any,
            val key: ByteArray,
            val reportFailure: () -> Unit,
        ) : Notice
        class Barrier(val origin: Any, val completion: CompletableDeferred<Unit>) : Notice
    }
    // Never closed: foreign CAS jobs may outlive a cancelled requester or closed execution.
    private val notices = Channel<Notice>(Channel.UNLIMITED)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    init {
        scope.launch {
            for (notice in notices) {
                when (notice) {
                    is Notice.Changed -> {
                        val failed = try { refresh(notice) } catch (_: Throwable) { true }
                        if (failed) {
                            failures[notice.origin] = true
                            runCatching { notice.reportFailure() }
                        }
                    }
                    is Notice.Barrier -> {
                        if (failures.remove(notice.origin) == true) {
                            notice.completion.completeExceptionally(HostRejection.Rejected("Permission refresh failed"))
                        } else {
                            notice.completion.complete(Unit)
                        }
                    }
                }
            }
        }
    }

    // Non-reentrant: `operation` must use the raw [HostCoreStorage], never `serialized` again.
    suspend fun <T> serialized(storage: HostCoreStorage, key: ByteArray, operation: suspend () -> T): T {
        val hash = 31 * storage.storageIdentifier.hashCode() + key.contentHashCode()
        return locks[hash and (locks.size - 1)].withLock { operation() }
    }

    fun register(runtime: TrUAPIHostRuntime, storageIdentifier: String) {
        synchronized(runtimes) { runtimes[runtime] = storageIdentifier }
    }

    fun unregister(runtime: TrUAPIHostRuntime) {
        synchronized(runtimes) { runtimes.remove(runtime) }
    }

    fun changed(storage: HostCoreStorage, origin: Any, key: ByteArray, reportFailure: () -> Unit) {
        notices.trySend(Notice.Changed(storage, origin, key.copyOf(), reportFailure)).getOrThrow()
    }

    suspend fun awaitChanges(origin: Any) {
        val completion = CompletableDeferred<Unit>()
        notices.trySend(Notice.Barrier(origin, completion)).getOrThrow()
        completion.await()
    }

    private suspend fun refresh(notice: Notice.Changed): Boolean {
        // CAS queues before completion to survive cancellation. Its physical write lock must
        // nevertheless be released before decoding metadata or entering any Rust core.
        serialized(notice.storage, notice.key) { }
        val description = nativeDescribeCoreStorageKey(notice.key)
        val request = description.permissionRequest ?: return false
        val productId = description.productId ?: return false
        val targets = synchronized(runtimes) {
            runtimes.entries.filter { (runtime, storageIdentifier) ->
                !runtime.isClosed && storageIdentifier == notice.storage.storageIdentifier
            }.map { it.key }
        }
        var failed = false
        for (runtime in targets) {
            if (runtime.isClosed) continue
            try {
                // Re-read the latest stored decision, including after its requesting execution closed.
                runtime.refreshPermissionAuthorization(productId, request)
            } catch (_: Throwable) {
                if (!runtime.isClosed) failed = true
            }
        }
        return failed
    }
}
