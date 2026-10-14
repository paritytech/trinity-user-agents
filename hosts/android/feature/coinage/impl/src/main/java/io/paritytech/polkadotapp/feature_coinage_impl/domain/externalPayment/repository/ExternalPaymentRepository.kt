package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.repository

import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.model.ExternalPayment
import kotlinx.coroutines.flow.Flow

interface ExternalPaymentRepository {
    /** Throws when a payment with the same key already exists. */
    suspend fun insert(payment: ExternalPayment)

    suspend fun exists(key: ExternalPaymentKey): Boolean

    suspend fun getNextPending(): ExternalPayment?

    fun observe(key: ExternalPaymentKey): Flow<ExternalPayment?>
}
