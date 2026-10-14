package io.paritytech.polkadotapp.app.root.navigation.fund

import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_fund_impl.FundRouter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetPayload
import javax.inject.Inject

class FundNavigator @Inject constructor(
    navigationHolder: NavigationHolder,
) : BaseNavigator(navigationHolder), FundRouter {
    override fun openFund(payload: AssetPayload) = performNavigation(
        actionId = R.id.action_selectAssetFragment_to_fundFragment,
        args = payload.toPayloadBundle()
    )

    override fun openConfirmationScreen() = performNavigation(R.id.action_fundFragment_to_confirmExitBottomSheetFragment)

    override fun exit() = performNavigation(R.id.action_finish_funding_flow)
}
