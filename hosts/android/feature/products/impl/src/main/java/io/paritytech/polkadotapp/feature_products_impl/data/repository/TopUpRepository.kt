package io.paritytech.polkadotapp.feature_products_impl.data.repository

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.database.dao.ProductTopUpDao
import io.paritytech.polkadotapp.database.model.ProductTopUpLocal
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.PaymentTopUpId
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpOperation
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpStatus
import java.math.BigInteger
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * The top-ups products have asked for.
 *
 * Everything a product may be told, and nothing secret: what a top-up is for, when its window opened, and —
 * once there is one — its verdict. A running top-up's status is not stored, because the ledger already has
 * it and would only be contradicted by a second copy.
 */
interface TopUpRepository {
    /** Fails when [operation] names an id the product has already used. */
    suspend fun insert(operation: TopUpOperation): Result<Unit>

    suspend fun get(productId: ProductId, id: PaymentTopUpId): TopUpOperation?

    /** The top-ups that have not reached a verdict, across every product. */
    suspend fun unfinished(): List<TopUpOperation>

    /** Writes the verdict. [outcome] must be terminal: nothing may change after this. */
    suspend fun settle(operation: TopUpOperation, outcome: TopUpStatus): Result<Unit>
}

@OptIn(ExperimentalTime::class)
class RealTopUpRepository @Inject constructor(
    private val dao: ProductTopUpDao,
) : TopUpRepository {
    override suspend fun insert(operation: TopUpOperation): Result<Unit> = runCancellableCatching {
        dao.insert(operation.toLocal())
    }

    override suspend fun get(productId: ProductId, id: PaymentTopUpId): TopUpOperation? =
        dao.get(productId.value, id.asHex())?.toDomain()

    override suspend fun unfinished(): List<TopUpOperation> = dao.getUnfinished().map { it.toDomain() }

    override suspend fun settle(operation: TopUpOperation, outcome: TopUpStatus): Result<Unit> =
        runCancellableCatching {
            dao.settle(
                productId = operation.productId.value,
                topUpId = operation.id.asHex(),
                outcome = outcome.toLocal(),
                actualClaimedPlanks = (outcome as? TopUpStatus.ClaimedPartially)?.actualClaimed?.value,
            )
        }
}

@OptIn(ExperimentalTime::class)
private fun TopUpOperation.toLocal() = ProductTopUpLocal(
    productId = productId.value,
    topUpId = id.asHex(),
    amountPlanks = amount.value,
    startedAtMillis = startedAt.toEpochMilliseconds(),
    outcome = outcome?.toLocal(),
    actualClaimedPlanks = (outcome as? TopUpStatus.ClaimedPartially)?.actualClaimed?.value,
)

@OptIn(ExperimentalTime::class)
private fun ProductTopUpLocal.toDomain() = TopUpOperation(
    id = PaymentTopUpId.fromHex(topUpId).getOrThrow(),
    productId = ProductId.fromStoredValue(productId),
    amount = amountPlanks.intoBalance(),
    startedAt = Instant.fromEpochMilliseconds(startedAtMillis),
    outcome = outcome?.toDomain(actualClaimedPlanks),
)

private fun TopUpStatus.toLocal(): ProductTopUpLocal.Outcome = when (this) {
    is TopUpStatus.Claimed -> ProductTopUpLocal.Outcome.CLAIMED
    is TopUpStatus.ClaimedPartially -> ProductTopUpLocal.Outcome.CLAIMED_PARTIALLY

    // Only a verdict is ever handed here, and neither of these is one.
    is TopUpStatus.NotClaimed, is TopUpStatus.Detecting, is TopUpStatus.Claiming ->
        ProductTopUpLocal.Outcome.NOT_CLAIMED
}

private fun ProductTopUpLocal.Outcome.toDomain(actualClaimedPlanks: BigInteger?): TopUpStatus = when (this) {
    // Only a finalized claim is ever recorded as a verdict, so it can only have been that one.
    ProductTopUpLocal.Outcome.CLAIMED -> TopUpStatus.Claimed(finalized = true)

    ProductTopUpLocal.Outcome.CLAIMED_PARTIALLY -> TopUpStatus.ClaimedPartially(
        requireNotNull(actualClaimedPlanks) { "partial top-up recorded without its amount" }.intoBalance()
    )

    ProductTopUpLocal.Outcome.NOT_CLAIMED -> TopUpStatus.NotClaimed
}
