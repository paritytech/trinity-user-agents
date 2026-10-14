package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.repository

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.database.dao.ExternalPaymentDao
import io.paritytech.polkadotapp.database.model.ExternalPaymentLocal
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.SelectedVoucherKeysCodec
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.model.ExternalPayment
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RealExternalPaymentRepository @Inject constructor(
    private val dao: ExternalPaymentDao,
    private val selectedVoucherKeysCodec: SelectedVoucherKeysCodec,
) : ExternalPaymentRepository {
    override suspend fun insert(payment: ExternalPayment) {
        dao.insert(payment.toLocal())
    }

    override suspend fun exists(key: ExternalPaymentKey): Boolean =
        dao.exists(key.origin, key.id)

    override suspend fun getNextPending(): ExternalPayment? =
        dao.getNextPending()?.toDomain()

    override fun observe(key: ExternalPaymentKey): Flow<ExternalPayment?> =
        dao.observeById(key.origin, key.id).map { it?.toDomain() }

    private fun ExternalPaymentLocal.toDomain() = ExternalPayment(
        key = ExternalPaymentKey(origin = origin, id = id),
        amount = amountPlanks.intoBalance(),
        destination = destination.intoAccountId(),
        stage = toDomainStage(selectedVoucherKeysCodec),
        createdAt = createdAt,
        updatedAt = updatedAt,
    )

    private fun ExternalPayment.toLocal(): ExternalPaymentLocal {
        val row = stage.toRow(selectedVoucherKeysCodec)
        return ExternalPaymentLocal(
            id = key.id,
            origin = key.origin,
            amountPlanks = amount.value,
            destination = destination.value,
            stage = row.stage,
            failureReason = row.failureReason,
            selectedVoucherKeys = row.selectedVoucherKeys,
            surplusPlanks = row.surplusPlanks,
            claimedPlanks = row.claimedPlanks,
            createdAt = createdAt,
            updatedAt = updatedAt,
        )
    }
}
