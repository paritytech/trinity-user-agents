package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import java.math.BigDecimal

/**
 * A payment of [amount], checked against the balance. [spendsGainingPrivacy] when it needs value still gaining
 * privacy, which the user confirms before it is sent.
 */
data class TransferPlan(
    val amount: BigDecimal,
    val spendsGainingPrivacy: Boolean,
)
