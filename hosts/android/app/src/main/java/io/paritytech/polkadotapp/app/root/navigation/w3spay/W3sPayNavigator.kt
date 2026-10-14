package io.paritytech.polkadotapp.app.root.navigation.w3spay

import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_w3spay_impl.W3sPayRouter
import io.paritytech.polkadotapp.feature_wallet_api.presentation.enterAmount.SendEnterAmountPayload
import javax.inject.Inject

class W3sPayNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
) : BaseNavigator(navigationHolder), W3sPayRouter {
    override fun openW3sPayment(payload: SendEnterAmountPayload) = performNavigation(
        actionId = R.id.action_global_to_send_enter_amount_graph,
        args = payload.toPayloadBundle()
    )
}
