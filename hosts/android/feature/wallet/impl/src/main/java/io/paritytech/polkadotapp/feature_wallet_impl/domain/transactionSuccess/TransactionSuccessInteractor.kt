package io.paritytech.polkadotapp.feature_wallet_impl.domain.transactionSuccess

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentState
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinagePaymentStatusUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.transformWhile
import javax.inject.Inject

class TransactionSuccessInteractor @Inject constructor(
    private val coinagePaymentStatusUseCase: CoinagePaymentStatusUseCase,
) {
    fun subscribeClaimsFinalized(coins: List<AccountId>): Flow<Boolean> =
        coinagePaymentStatusUseCase.subscribeStatuses(coins)
            .map { states -> states.allClaimsFinalized() }
            .distinctUntilChanged()
            .transformWhile { finalized ->
                emit(finalized)
                !finalized
            }

    private fun Map<AccountId, CoinagePaymentState>.allClaimsFinalized(): Boolean =
        isNotEmpty() && values.all { it.status == CoinagePaymentStatus.Claimed(finalized = true) }
}
