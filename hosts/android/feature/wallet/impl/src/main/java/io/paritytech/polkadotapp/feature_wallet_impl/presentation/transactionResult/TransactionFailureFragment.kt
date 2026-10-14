package io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult

import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.TransactionOutcomeFragment
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.compose.TransactionOutcomeUiConfig

@AndroidEntryPoint
class TransactionFailureFragment :
    TransactionOutcomeFragment<TransactionFailureViewModel>() {
    override val viewModel: TransactionFailureViewModel by viewModels()

    override fun config(): TransactionOutcomeUiConfig = TransactionOutcomeUiConfig.failure(
        title = R.string.send_transaction_failure_title,
        message = R.string.send_transaction_failure_description,
        buttonText = R.string.common_retry
    )
}
