package io.paritytech.polkadotapp.feature_products_impl.data.repository

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.database.dao.ProductFundingOperationDao
import io.paritytech.polkadotapp.database.model.ProductFundingOperationLocal
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.operation.OperationId
import javax.inject.Inject

data class FundingOperationRecord(
    val productId: ProductId,
    val id: OperationId,
    val label: String?,
)

/**
 * Persistence for open funding operations. Behind an interface so the storage backing can change
 * without touching the operation service. Records outlive the process so a funding flow can resume
 * after a restart; they are deleted when the operation ends.
 */
interface ProductFundingOperationRepository {
    /** Persists a new open operation and returns its assigned id. */
    suspend fun insert(productId: ProductId, label: String?): Result<OperationId>
    suspend fun delete(id: OperationId): Result<Unit>
    suspend fun loadAll(): List<FundingOperationRecord>
}

class RealProductFundingOperationRepository @Inject constructor(
    private val dao: ProductFundingOperationDao,
) : ProductFundingOperationRepository {
    override suspend fun insert(productId: ProductId, label: String?): Result<OperationId> = runCancellableCatching {
        val rowId = dao.insert(ProductFundingOperationLocal(operationId = 0, productId = productId.value, label = label))
        OperationId(rowId)
    }

    override suspend fun delete(id: OperationId): Result<Unit> = runCancellableCatching {
        dao.delete(id.value)
    }

    override suspend fun loadAll(): List<FundingOperationRecord> = dao.getAll().map {
        FundingOperationRecord(ProductId.fromStoredValue(it.productId), OperationId(it.operationId), it.label)
    }
}
