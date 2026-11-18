package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.domain

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageExportId

sealed interface SendState {
    /** Submitted, but the coins are not on chain yet. */
    data object Detecting : SendState

    /** On chain and waiting for the recipient to take them. */
    data object Detected : SendState

    /** [unfinalizedExport] is the payment whose claim is still only in a best block, or null when nothing is left to await. */
    data class Complete(val unfinalizedExport: CoinageExportId?) : SendState

    data class Failed(val error: Throwable) : SendState
}
