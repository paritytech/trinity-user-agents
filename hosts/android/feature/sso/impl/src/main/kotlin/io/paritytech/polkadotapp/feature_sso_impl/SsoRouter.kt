package io.paritytech.polkadotapp.feature_sso_impl

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.PairRequestPayload

interface SsoRouter : ReturnableRouter {
    fun openPairRequest(payload: PairRequestPayload)
}
