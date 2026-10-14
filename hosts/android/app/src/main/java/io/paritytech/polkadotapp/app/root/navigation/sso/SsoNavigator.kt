package io.paritytech.polkadotapp.app.root.navigation.sso

import io.paritytech.polkadotapp.app.R
import io.paritytech.polkadotapp.app.root.navigation.BaseNavigator
import io.paritytech.polkadotapp.app.root.navigation.NavigationHolder
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_sso_impl.SsoRouter
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.PairRequestPayload
import javax.inject.Inject

class SsoNavigator @Inject constructor(
    navigationHolder: NavigationHolder
) : BaseNavigator(navigationHolder), SsoRouter {
    override fun openPairRequest(payload: PairRequestPayload) {
        performNavigation(
            actionId = R.id.action_global_to_pairRequestBottomSheet,
            args = payload.toPayloadBundle()
        )
    }
}
