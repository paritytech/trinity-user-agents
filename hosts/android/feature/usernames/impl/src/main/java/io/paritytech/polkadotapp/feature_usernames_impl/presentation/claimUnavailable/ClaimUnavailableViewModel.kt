package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claimUnavailable

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.UsernamesRouter
import javax.inject.Inject

@HiltViewModel
class ClaimUnavailableViewModel @Inject constructor(
    private val router: UsernamesRouter
) : BaseViewModel() {
    fun onRecoverClicked() {
        router.openRecoverOptionsFromClaimUnavailable()
    }

    fun backPressed() {
        router.back()
    }
}
