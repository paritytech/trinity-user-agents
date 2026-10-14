package io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.compose

import androidx.compose.ui.graphics.vector.ImageVector
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Failure
import io.paritytech.polkadotapp.design.components.icon.vectors.Success

class TransactionOutcomeUiConfig(
    val icon: ImageVector,
    val titleRes: Int,
    val messageRes: Int?,
    val buttonRes: Int,
) {
    companion object {
        fun success(
            title: Int,
            message: Int?,
            buttonText: Int,
        ) = TransactionOutcomeUiConfig(
            icon = NovaIcons.Success,
            titleRes = title,
            messageRes = message,
            buttonRes = buttonText
        )

        fun defaultSuccess() = success(
            title = R.string.transaction_success_title,
            message = null,
            buttonText = R.string.common_done
        )

        fun failure(
            title: Int,
            message: Int?,
            buttonText: Int,
        ) = TransactionOutcomeUiConfig(
            icon = NovaIcons.Failure,
            titleRes = title,
            messageRes = message,
            buttonRes = buttonText
        )
    }
}
