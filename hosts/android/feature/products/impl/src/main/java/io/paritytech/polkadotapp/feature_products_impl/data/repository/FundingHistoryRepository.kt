package io.paritytech.polkadotapp.feature_products_impl.data.repository

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.database.dao.ProductFundingRecordDao
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.history.FundingRecord
import javax.inject.Inject

/** The ended funding sessions the host keeps once the core lets go of them. */
interface FundingHistoryRepository {
    /** Writes [record], replacing the row for its session when there is one. */
    suspend fun save(record: FundingRecord): Result<Unit>

    suspend fun records(): Result<List<FundingRecord>>
}

class RealFundingHistoryRepository @Inject constructor(
    private val dao: ProductFundingRecordDao,
) : FundingHistoryRepository {
    override suspend fun save(record: FundingRecord): Result<Unit> = runCancellableCatching {
        dao.upsert(record.toLocal())
    }

    override suspend fun records(): Result<List<FundingRecord>> = runCancellableCatching {
        dao.getAll().map { it.toRecord() }
    }
}
