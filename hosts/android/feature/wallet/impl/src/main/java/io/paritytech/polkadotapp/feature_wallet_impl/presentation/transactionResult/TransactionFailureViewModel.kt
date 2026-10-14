package io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.TransactionOutcomeViewModel
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import javax.inject.Inject

@HiltViewModel
class TransactionFailureViewModel @Inject constructor(
    private val pocketRouter: PocketRouter,
) : TransactionOutcomeViewModel() {
    override fun onButtonClick() {
        pocketRouter.back()
    }

    override fun onBackClick() {
        pocketRouter.back()
    }
}
