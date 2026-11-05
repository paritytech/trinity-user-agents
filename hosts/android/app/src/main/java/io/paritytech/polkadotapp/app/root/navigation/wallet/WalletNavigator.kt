package io.paritytech.polkadotapp.app.root.navigation.wallet

import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaSheetPayload
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult.TransactionSuccessPayload
import javax.inject.Inject

class PocketNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
) : BaseNavigator(navigationHolder), PocketRouter {
    override fun openSendPayment() =
        performNavigation(R.id.action_global_to_send_contact_payment_graph)

    override fun openSendEnterAmount(payload: SendEnterAmountPayload) = performNavigation(
        actionId = R.id.action_sendPaymentFragment_to_send_enter_amount_graph,
        args = payload.toPayloadBundle()
    )

    override fun openSendEnterAmountFromDeeplink(payload: SendEnterAmountPayload) = performNavigation(
        actionId = R.id.action_global_to_send_enter_amount_graph,
        args = payload.toPayloadBundle()
    )

    override fun openSuccess(payload: TransactionSuccessPayload) = performNavigation(
        actionId = R.id.action_sendEnterAmountFragment_to_transactionSuccessFragment,
        args = payload.toPayloadBundle()
    )

    override fun openFailure() =
        performNavigation(R.id.action_sendEnterAmountFragment_to_transactionFailureFragment)

    override fun openScanAddressQr() =
        performNavigation(R.id.action_sendPaymentFragment_to_scanAddressQrFragment)

    override fun openSpaSheet(url: String) {
        if (isCurrentDestination(R.id.spaSheetBottomSheet)) return
        performNavigation(
            actionId = R.id.action_global_to_spaSheetBottomSheet,
            args = SpaSheetPayload(url).toPayloadBundle()
        )
    }
}
