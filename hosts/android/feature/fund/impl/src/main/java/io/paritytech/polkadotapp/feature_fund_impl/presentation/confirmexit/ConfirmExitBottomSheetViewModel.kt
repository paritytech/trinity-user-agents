package io.paritytech.polkadotapp.feature_fund_impl.presentation.confirmexit

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_fund_impl.FundRouter
import javax.inject.Inject

@HiltViewModel
class ConfirmExitBottomSheetViewModel @Inject constructor(
    private val router: FundRouter,
) : BaseViewModel(), ConfirmExitContract {
    override fun confirmClicked() {
        router.exit()
    }

    override fun dismissClicked() {
        router.back()
    }
}
