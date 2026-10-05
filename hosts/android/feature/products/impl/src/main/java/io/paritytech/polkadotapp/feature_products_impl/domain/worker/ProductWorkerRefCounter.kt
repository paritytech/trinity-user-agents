package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import dagger.Lazy
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.FixedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Owns product worker lifecycle by reference count. Acquiring the first reference to a product
 * starts its worker; releasing the last disposes it. References in between neither restart nor
 * re-dispose it.
 */
interface ProductWorkerRefCounter {
    suspend fun acquire(productId: ProductId, label: String): ProductWorkerReference
}

interface ProductWorkerReference {
    /** Awaits boot and returns the running worker. Throws if this reference is released or the product has no worker. */
    suspend fun worker(): ProductWorker

    /** Binds a modality API (e.g. chat messaging) onto the shared worker for as long as this reference is held. */
    suspend fun enableModalityApi(api: WorkerModalityApi)

    /** Releases this reference. Idempotent. */
    fun release()
}

suspend fun <R> ProductWorkerRefCounter.withWorkerAcquired(
    productId: ProductId,
    label: String,
    block: suspend (ProductWorkerReference) -> R,
): R {
    val reference = acquire(productId, label)
    return try {
        block(reference)
    } finally {
        reference.release()
    }
}

@Singleton
class RealProductWorkerRefCounter @Inject constructor(
    private val hostApiInteractor: HostApiInteractor,
    // Lazy breaks the Dagger cycle: the boot factory builds a worker whose host calls reach back into
    // this ref counter (via the operation service), so it must not be constructed eagerly here.
    private val bootFactory: Lazy<WorkerBootFactory>,
    private val runtimeSettings: io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings,
    private val runtimeProvider: Lazy<io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider>,
    private val rustWorkers: Lazy<io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIWorkerSupervisor>,
    dispatchers: CoroutineDispatchers,
) : ProductWorkerRefCounter, CoroutineScope by CoroutineScope(SupervisorJob() + dispatchers.computation) {
    private val handles = ConcurrentHashMap<ProductId, ProductWorkerHandle>()

    override suspend fun acquire(productId: ProductId, label: String): ProductWorkerReference {
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) return acquireRustWorker(productId)
        return handles.computeIfAbsent(productId) { ProductWorkerHandle(it) }.acquire(label)
    }

    private suspend fun acquireRustWorker(productId: ProductId): ProductWorkerReference {
        val runtime = runtimeProvider.get().runtime().getOrThrow()
        runtime.acquireWorker(productId.value)
        val workers = rustWorkers.get()
        return object : ProductWorkerReference {
            private val released = AtomicBoolean(false)
            private var roomsJob: kotlinx.coroutines.Job? = null
            private val productWorker = io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIProductWorker(productId, workers)

            override suspend fun worker(): ProductWorker {
                check(!released.get()) { "Worker reference is released" }
                return productWorker
            }

            override suspend fun enableModalityApi(api: WorkerModalityApi) {
                check(!released.get()) { "Worker reference is released" }
                when (api) {
                    is WorkerModalityApi.Chat -> {
                        val chat = workers.chat(productId)
                        chat.messaging = api.messaging
                        roomsJob?.cancel()
                        roomsJob = launch {
                            api.messaging.subscribeChatRooms().collect {
                                workers.currentExecution(productId)?.notifyChatRoomsChanged(chat.listRooms())
                            }
                        }
                    }
                }
            }

            override fun release() {
                if (!released.compareAndSet(false, true)) return
                roomsJob?.cancel()
                runtime.releaseWorker(productId.value)
            }
        }
    }

    private sealed interface WorkerState {
        data object NotBooted : WorkerState
        data object NoWorker : WorkerState
        data class Booted(val worker: ProductWorker) : WorkerState
    }

    // The permanent per-product anchor. One collector on its ref-count flow is the single serialized
    // lifecycle chain, so rapid acquire and release can never orphan a worker; started count always
    // converges to disposed count. The handle is never removed, so the chain keeps one anchor.
    private inner class ProductWorkerHandle(private val productId: ProductId) {
        private val refCount = MutableStateFlow(0)
        private val workerState = MutableStateFlow<WorkerState>(WorkerState.NotBooted)
        private val botApi = BindableProductsBotApi(hostApiInteractor, FixedProductId(productId))

        private var bootScope: CoroutineScope? = null

        init {
            launch {
                refCount.map { it > 0 }
                    .distinctUntilChanged()
                    .collect { wanted -> if (wanted) boot() else dispose() }
            }
        }

        fun acquire(label: String): ProductWorkerReference {
            refCount.update { it + 1 }
            Timber.d("Acquired worker for $productId by '$label', refCount=${refCount.value}")
            return Reference(label)
        }

        private suspend fun awaitWorker(): ProductWorker {
            val state = workerState.filter { it !is WorkerState.NotBooted }.first()
            return (state as? WorkerState.Booted)?.worker
                ?: error("Product $productId publishes no worker to drive")
        }

        private suspend fun boot() {
            if (bootScope != null) return
            val scope = this@RealProductWorkerRefCounter.childScope(supervised = true)
            bootScope = scope

            val worker = runCatching { bootFactory.get().boot(productId, botApi, scope) }
                .onFailure { Timber.e(it, "Worker boot failed for product $productId") }
                .getOrNull()

            if (worker == null) {
                scope.cancel()
                bootScope = null
                workerState.value = WorkerState.NoWorker
                return
            }

            // A fast acquire-then-release may have dropped the count to zero while we were booting.
            // Dispose what we built rather than keep an unwanted worker.
            if (refCount.value == 0) {
                scope.cancel()
                bootScope = null
                workerState.value = WorkerState.NotBooted
                return
            }

            workerState.value = WorkerState.Booted(worker)
        }

        private fun dispose() {
            val scope = bootScope ?: return
            scope.cancel()
            bootScope = null
            workerState.value = WorkerState.NotBooted
        }

        private inner class Reference(private val label: String) : ProductWorkerReference {
            private val released = AtomicBoolean(false)
            private var boundChat = false

            override suspend fun worker(): ProductWorker {
                check(!released.get()) { "Worker reference for $productId already released" }
                return awaitWorker()
            }

            override suspend fun enableModalityApi(api: WorkerModalityApi) {
                check(!released.get()) { "Worker reference for $productId already released" }
                when (api) {
                    is WorkerModalityApi.Chat -> {
                        botApi.chatSlot.set(api.messaging)
                        boundChat = true
                    }
                }
            }

            override fun release() {
                if (!released.compareAndSet(false, true)) return
                if (boundChat) botApi.chatSlot.clear()
                refCount.update { (it - 1).coerceAtLeast(0) }
                Timber.d("Released worker for $productId by '$label', refCount=${refCount.value}")
            }
        }
    }
}
