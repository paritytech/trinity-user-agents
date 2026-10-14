package io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.TransactionOutcomeViewModel
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.domain.transactionSuccess.TransactionSuccessInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult.TransactionSuccessUiState.Finality
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import timber.log.Timber
import javax.inject.Inject

@HiltViewModel
class TransactionSuccessViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val walletRouter: PocketRouter,
    private val interactor: TransactionSuccessInteractor,
) : TransactionOutcomeViewModel(), TransactionSuccessContract {
    private val payload: TransactionSuccessPayload = savedStateHandle.getPayload()
    private val unfinalizedCoins = payload.unfinalizedCoins?.map { it.intoAccountId() }

    private val finalityFlow: Flow<Finality?> = if (unfinalizedCoins == null) {
        flowOf(null)
    } else {
        interactor.subscribeClaimsFinalized(unfinalizedCoins)
            .map<Boolean, Finality?> { finalized -> if (finalized) Finality.CONFIRMED else Finality.PENDING }
            .catch { error ->
                Timber.e(error, "Failed to watch payment finality")
                emit(null)
            }
    }

    override val state: StateFlow<TransactionSuccessUiState> = finalityFlow
        .map(::TransactionSuccessUiState)
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = TransactionSuccessUiState(finality = unfinalizedCoins?.let { Finality.PENDING })
        )

    override fun onButtonClick() {
        walletRouter.back()
    }

    override fun onBackClick() {
        walletRouter.back()
    }
}
