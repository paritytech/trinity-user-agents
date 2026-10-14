package io.paritytech.polkadotapp.feature_w3spay_impl

import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload

interface W3sPayRouter {
    /**
     * Opens the existing SendEnterAmount payment screen for a W3S real-time payment. The [payload]
     * carries a [io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.TransferMethodPayload.CoinsViaStatementStore]
     * transfer method with the amount pre-filled and locked.
     */
    fun openW3sPayment(payload: SendEnterAmountPayload)
}
