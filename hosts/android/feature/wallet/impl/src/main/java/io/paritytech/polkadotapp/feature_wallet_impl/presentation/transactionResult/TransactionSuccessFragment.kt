package io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.fragment.app.viewModels
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.TransactionOutcomeFragment
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.compose.TransactionOutcomeScreen
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.compose.TransactionOutcomeUiConfig
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult.compose.PaymentFinalityLabel

@AndroidEntryPoint
class TransactionSuccessFragment :
    TransactionOutcomeFragment<TransactionSuccessViewModel>() {
    override val viewModel: TransactionSuccessViewModel by viewModels()

    override fun config(): TransactionOutcomeUiConfig = TransactionOutcomeUiConfig.success(
        title = R.string.transaction_success_title,
        message = null,
        buttonText = R.string.common_done
    )

    @Composable
    override fun Screen() {
        val state by viewModel.state.collectAsStateWithLifecycle()

        TransactionOutcomeScreen(viewModel, config()) {
            val finality = state.finality
            if (finality != null) {
                VerticalSpacer { mediumIncreased }

                PaymentFinalityLabel(finality = finality)
            }
        }
    }
}
