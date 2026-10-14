package io.paritytech.polkadotapp.feature_products_impl.domain.operation

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductFundingOperationRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerReference
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

@JvmInline
value class OperationId(val value: Long)

/**
 * Backs workerBeginOperation / workerEndOperation. An open operation keeps the product's worker
 * alive (a held [ProductWorkerReference]) and is persisted so it resumes after an app restart via
 * [resumeOpenOperations].
 */
interface ProductOperationService {
    suspend fun begin(productId: ProductId, label: String?): Result<OperationId>

    /** Idempotent: an unknown or already-ended id is a successful no-op. */
    suspend fun end(productId: ProductId, id: OperationId): Result<Unit>

    /** Re-acquires the worker for every operation left open by a previous run. Called once at startup. */
    suspend fun resumeOpenOperations()
}

@Singleton
class RealProductOperationService @Inject constructor(
    private val workerRefCounter: ProductWorkerRefCounter,
    private val repository: ProductFundingOperationRepository,
) : ProductOperationService {
    private data class OperationKey(val productId: ProductId, val id: OperationId)

    private val mutex = Mutex()
    private val activeReferences = mutableMapOf<OperationKey, ProductWorkerReference>()

    override suspend fun begin(productId: ProductId, label: String?): Result<OperationId> {
        val reference = workerRefCounter.acquire(productId, label = "operation:${productId.value}")

        var stored = false
        try {
            return repository.insert(productId, label).map { id ->
                mutex.withLock { activeReferences[OperationKey(productId, id)] = reference }
                stored = true
                id
            }
        } finally {
            // A failed persist or a cancellation between acquire and store must never pin a worker.
            if (!stored) reference.release()
        }
    }

    override suspend fun end(productId: ProductId, id: OperationId): Result<Unit> {
        val reference = mutex.withLock { activeReferences.remove(OperationKey(productId, id)) }
            ?: return Result.success(Unit)

        // Always release the worker, even if the best-effort record delete fails.
        reference.release()
        repository.delete(id).logFailure("Failed to delete funding operation ${id.value} for $productId")
        return Result.success(Unit)
    }

    override suspend fun resumeOpenOperations() {
        val records = repository.loadAll()
        records.forEach { record ->
            val key = OperationKey(record.productId, record.id)
            val reference = workerRefCounter.acquire(record.productId, label = "operation:${record.productId.value}")
            val stored = mutex.withLock {
                if (activeReferences.containsKey(key)) {
                    false
                } else {
                    activeReferences[key] = reference
                    true
                }
            }
            if (!stored) reference.release()
        }
    }
}
