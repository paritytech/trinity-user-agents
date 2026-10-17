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
import uniffi.truapi.NativeTrUApiHostRuntime
import uniffi.truapi.nativeDescribeCoreStorageKey

/** One coordinator for every adapter in this process, including independent Rust roots. */
internal object CoreStorageAccess {
    // Collisions only serialize unrelated slots. The bounded table never forgets a held slot lock.
    private val locks = Array(256) { Mutex() }
    private data class Owner(
        val storageIdentifier: String,
        val group: Any,
        val productId: String,
        val runtime: NativeTrUApiHostRuntime,
    )
    private val executions = WeakHashMap<TrUAPIProductExecution, Owner>()
    private val failures = WeakHashMap<Any, Boolean>()
    private sealed interface Notice {
        class Changed(
            val storage: HostCoreStorage,
            val origin: Any,
            val key: ByteArray,
            val reportFailure: () -> Unit,
            val authorizationChanged: (String) -> Unit,
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

    fun register(
        execution: TrUAPIProductExecution,
        storageIdentifier: String,
        group: Any,
        productId: String,
        runtime: NativeTrUApiHostRuntime,
    ) {
        synchronized(executions) { executions[execution] = Owner(storageIdentifier, group, productId, runtime) }
    }

    fun unregister(execution: TrUAPIProductExecution) {
        synchronized(executions) { executions.remove(execution) }
    }

    fun changed(
        storage: HostCoreStorage,
        origin: Any,
        key: ByteArray,
        reportFailure: () -> Unit,
        authorizationChanged: (String) -> Unit,
    ) {
        notices.trySend(Notice.Changed(storage, origin, key.copyOf(), reportFailure, authorizationChanged)).getOrThrow()
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
            }.map { it.value }.distinctBy { it.group }
        }
        var failed = false
        for (owner in targets) {
            try {
                // Canonical revocation may already have closed the execution. Refresh through
                // its process authority, which re-reads storage and fences the exact product scope.
                // Include the writer: its CAS can outlive the requesting Rust future.
                owner.runtime.refreshPermissionAuthorization(productId, request)
            } catch (_: Throwable) {
                // Do not suppress an unreadable policy or turn it into broader execution closure.
                failed = true
            }
        }
        // Notify after refresh so the shell sees canonical closure, including a dropped CAS caller.
        runCatching { notice.authorizationChanged(productId) }
        return failed
    }
}
