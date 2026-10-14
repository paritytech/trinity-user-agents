package io.paritytech.polkadotapp.feature_usernames_impl.presentation.integrityFailed

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.UsernamesRouter
import javax.inject.Inject

@HiltViewModel
class IntegrityFailedViewModel @Inject constructor(
    private val router: UsernamesRouter
) : BaseViewModel() {
    fun backPressed() {
        router.back()
    }
}
