package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import dagger.Lazy
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.BindableProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.FixedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
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

    fun chatMessaging(productId: ProductId): ProductChatMessaging
}

interface ProductWorkerReference {
    /** Awaits boot and returns the running worker. Throws if this reference is released or the boot failed. */
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
    dispatchers: CoroutineDispatchers,
) : ProductWorkerRefCounter, CoroutineScope by CoroutineScope(SupervisorJob() + dispatchers.computation) {
    private val handles = ConcurrentHashMap<ProductId, ProductWorkerHandle>()

    override suspend fun acquire(productId: ProductId, label: String): ProductWorkerReference {
        return handle(productId).acquire(label)
    }

    override fun chatMessaging(productId: ProductId): ProductChatMessaging = handle(productId).botApi

    private fun handle(productId: ProductId) = handles.computeIfAbsent(productId) { ProductWorkerHandle(it) }

    private sealed interface WorkerState {
        data object NotBooted : WorkerState
        data class NoWorker(val cause: Throwable) : WorkerState
        data class Booted(val worker: ProductWorker) : WorkerState
    }

    // The permanent per-product anchor. One collector on its ref-count flow is the single serialized
    // lifecycle chain, so rapid acquire and release can never orphan a worker; started count always
    // converges to disposed count. The handle is never removed, so the chain keeps one anchor.
    private inner class ProductWorkerHandle(private val productId: ProductId) {
        private val refCount = MutableStateFlow(0)
        private val demand = MutableStateFlow(0L)
        private val workerState = MutableStateFlow<WorkerState>(WorkerState.NotBooted)
        val botApi = BindableProductsBotApi(hostApiInteractor, FixedProductId(productId))

        private var bootScope: CoroutineScope? = null

        init {
            launch {
                // Ticketed, not edge-triggered: a release and re-acquire in the same tick leave the
                // count unchanged, and the chain would never reconcile the product again.
                demand.collect { if (refCount.value > 0) boot() else dispose() }
            }
        }

        fun acquire(label: String): ProductWorkerReference {
            refCount.update { it + 1 }
            demand.update { it + 1 }
            Timber.d("Acquired worker for $productId by '$label', refCount=${refCount.value}")
            return Reference(label)
        }

        private suspend fun awaitWorker(): ProductWorker {
            val state = workerState.filter { it !is WorkerState.NotBooted }.first()
            return (state as? WorkerState.Booted)?.worker
                ?: throw IllegalStateException(
                    "No worker booted for product $productId",
                    (state as? WorkerState.NoWorker)?.cause,
                )
        }

        private suspend fun boot() {
            if (bootScope != null) return
            // A previous failure must not be replayed to whoever waits on this attempt.
            workerState.value = WorkerState.NotBooted
            val scope = this@RealProductWorkerRefCounter.childScope(supervised = true)
            bootScope = scope

            val outcome = runCatching { bootFactory.get().boot(productId, botApi, scope) }
            val failure = outcome.exceptionOrNull()
            // Only our own cancellation ends this collector; anything else is a boot failure.
            if (failure is CancellationException && !currentCoroutineContext().isActive) {
                abandonBoot(scope)
                throw failure
            }
            if (failure != null) {
                Timber.e(failure, "Worker boot failed for product $productId")
                abandonBoot(scope)
                workerState.value = WorkerState.NoWorker(failure)
                return
            }
            val worker = outcome.getOrThrow()

            // A fast acquire-then-release may have dropped the count to zero while we were booting.
            // Dispose what we built rather than keep an unwanted worker.
            if (refCount.value == 0) {
                abandonBoot(scope)
                workerState.value = WorkerState.NotBooted
                return
            }

            workerState.value = WorkerState.Booted(worker)
        }

        private fun abandonBoot(scope: CoroutineScope) {
            scope.cancel()
            bootScope = null
        }

        private fun dispose() {
            bootScope?.cancel()
            bootScope = null
            workerState.value = WorkerState.NotBooted
        }

        private inner class Reference(private val label: String) : ProductWorkerReference {
            private val released = AtomicBoolean(false)
            private var boundChat: ProductChatMessaging? = null

            override suspend fun worker(): ProductWorker {
                check(!released.get()) { "Worker reference for $productId already released" }
                return awaitWorker()
            }

            override suspend fun enableModalityApi(api: WorkerModalityApi) {
                check(!released.get()) { "Worker reference for $productId already released" }
                when (api) {
                    is WorkerModalityApi.Chat -> {
                        botApi.chatSlot.set(api.messaging)
                        boundChat = api.messaging
                    }
                }
            }

            override fun release() {
                if (!released.compareAndSet(false, true)) return
                boundChat?.let(botApi.chatSlot::clear)
                refCount.update { (it - 1).coerceAtLeast(0) }
                demand.update { it + 1 }
                Timber.d("Released worker for $productId by '$label', refCount=${refCount.value}")
            }
        }
    }
}
