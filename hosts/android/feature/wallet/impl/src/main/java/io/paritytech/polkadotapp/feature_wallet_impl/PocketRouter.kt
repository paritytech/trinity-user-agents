package io.paritytech.polkadotapp.feature_wallet_impl

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult.TransactionSuccessPayload

interface PocketRouter : ReturnableRouter {
    fun openSendPayment()

    fun openSendEnterAmount(payload: SendEnterAmountPayload)

    fun openSendEnterAmountFromDeeplink(payload: SendEnterAmountPayload)

    fun openSuccess(payload: TransactionSuccessPayload)

    fun openFailure()

    fun openScanAddressQr()

    fun openSpaSheet(url: String)
}
