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
    private data class Owner(val storageIdentifier: String, val group: Any, val productId: String)
    private val executions = WeakHashMap<TrUAPIProductExecution, Owner>()
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

    fun register(execution: TrUAPIProductExecution, storageIdentifier: String, group: Any, productId: String) {
        synchronized(executions) { executions[execution] = Owner(storageIdentifier, group, productId) }
    }

    fun unregister(execution: TrUAPIProductExecution) {
        synchronized(executions) { executions.remove(execution) }
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
        val targets = synchronized(executions) {
            executions.entries.filter { (execution, owner) ->
                !execution.isClosed && owner.storageIdentifier == notice.storage.storageIdentifier &&
                    owner.productId == productId
            }.map { it.key to it.value.group }.groupBy({ it.second }, { it.first })
        }
        var failed = false
        for (group in targets.values) {
            for (execution in group) {
                if (execution.isClosed) continue
                try {
                    // A shared root refresh fences every matching execution in that root.
                    // It re-reads storage; a delayed notice never replays an old decision.
                    // Include the writer: its CAS can outlive the requesting Rust future.
                    // A latest stored grant is a core no-op, not a new revision.
                    execution.refreshPermissionAuthorization(request)
                    break
                } catch (_: Throwable) {
                    if (execution.isClosed) continue
                    // Rust already fail-closes only the exact affected permission scope.
                    // Closing the whole execution here could kill another account's call.
                    failed = true
                    break
                }
            }
        }
        return failed
    }
}
