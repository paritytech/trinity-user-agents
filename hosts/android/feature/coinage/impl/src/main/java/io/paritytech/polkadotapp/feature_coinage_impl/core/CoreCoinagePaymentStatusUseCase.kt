package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageExportId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatusUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.ExportStatus
import uniffi.truapi.ExportedCoinStatus
import uniffi.truapi.NativeExportStatusObserver
import javax.inject.Inject

class CoreCoinagePaymentStatusUseCase @Inject constructor(
    private val core: CoinageCore,
) : CoinagePaymentStatusUseCase {
    override fun subscribeStatuses(exportId: CoinageExportId): Flow<List<CoinagePaymentState>> = flow {
        val units = core.units().getOrElse { throw it }
        val coins = core.observe<List<ExportedCoinStatus>> { coinage, push ->
            coinage.observeExportStatus(
                exportId.value.value,
                object : NativeExportStatusObserver {
                    override fun onExportStatus(coins: List<ExportedCoinStatus>) = push(coins)
                }
            )
        }
        emitAll(coins.map { statuses -> statuses.map { it.toPaymentState(units) } })
    }
}

private fun ExportedCoinStatus.toPaymentState(units: CoinageUnits) = CoinagePaymentState(
    account = account.intoAccountId(),
    value = units.balanceOfExponent(exponent),
    status = when (val status = status) {
        ExportStatus.NotYetMinted -> CoinagePaymentStatus.Detecting
        ExportStatus.AwaitingPayee -> CoinagePaymentStatus.AwaitingClaim
        is ExportStatus.Claimed -> CoinagePaymentStatus.Claimed(finalized = status.stable)
        is ExportStatus.Reclaimed -> if (status.stable) CoinagePaymentStatus.Failed else CoinagePaymentStatus.Detecting
        ExportStatus.NeverMinted -> CoinagePaymentStatus.Failed
    },
)
