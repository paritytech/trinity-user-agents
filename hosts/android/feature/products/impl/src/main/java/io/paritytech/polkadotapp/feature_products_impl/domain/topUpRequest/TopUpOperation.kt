package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * A top-up a product asked for, as everything but its source sees it.
 *
 * [startedAt] is what makes the retry window belong to the operation rather than to whichever process
 * happens to be running it: a resumed top-up finishes the window it was given, and is never handed another.
 *
 * [outcome] is null until nothing further will be attempted, and is a terminal status from then on.
 */
@OptIn(ExperimentalTime::class)
data class TopUpOperation(
    val id: PaymentTopUpId,
    val productId: ProductId,
    val amount: Balance,
    val startedAt: Instant,
    val outcome: TopUpStatus?,
) {
    /**
     * The coinage group its transactions are registered under, so a resumed run rejoins them rather than
     * paying twice.
     *
     * Derived rather than stored, because a second copy of something the operation already determines can
     * only ever disagree with it. Qualified by the product because the id is a product's own opaque bytes:
     * two products may each register the same one, and a shared group would let each product's transactions
     * count towards the other's total.
     */
    val groupId: CoinageOperationGroupId
        get() = CoinageOperationGroupId("$TOP_UP_GROUP_PREFIX${productId.value}:${id.asHex()}")
}

private const val TOP_UP_GROUP_PREFIX = "top up:"
