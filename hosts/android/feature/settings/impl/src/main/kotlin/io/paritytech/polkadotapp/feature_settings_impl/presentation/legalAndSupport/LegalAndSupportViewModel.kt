package io.paritytech.polkadotapp.feature_settings_impl.presentation.legalAndSupport

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import javax.inject.Inject

@HiltViewModel
class LegalAndSupportViewModel @Inject constructor(
    private val router: SettingsRouter
) : BaseViewModel(), LegalAndSupportContract {
    override fun onBackClick() {
        router.back()
    }

    override fun onPrivacyPolicyClick() {
        router.openPrivacyPolicy()
    }

    override fun onTermsOfUseClick() {
        router.openTermsOfUse()
    }

    override fun onContactUsClick() {
        router.openContactUs()
    }
}
