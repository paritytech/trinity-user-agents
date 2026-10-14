package io.paritytech.polkadotapp.feature_fund_impl

import io.paritytech.polkadotapp.common.presentation.navigation.ReturnableRouter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetPayload

interface FundRouter : ReturnableRouter {
    fun openFund(payload: AssetPayload)

    fun openConfirmationScreen()

    fun exit()
}
