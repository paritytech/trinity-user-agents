package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId

/**
 * Derived from the payment key, so a group is found again after a crash without anything extra being persisted
 * for it.
 */
object ExternalPaymentGroupIds {
    fun recycling(key: ExternalPaymentKey) = CoinageOperationGroupId("external-payment-recycle:${key.origin}:${key.id}")

    fun unload(key: ExternalPaymentKey) = CoinageOperationGroupId("external-payment:${key.origin}:${key.id}")
}
