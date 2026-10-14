package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.mnemonic

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.stateInBackground
import io.paritytech.polkadotapp.design.components.mnemonic.model.toWordList
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.MnemonicRevealInteractor
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import javax.inject.Inject

@HiltViewModel
class MnemonicRevealViewModel @Inject constructor(
    private val router: SettingsRouter,
    private val interactor: MnemonicRevealInteractor
) : BaseViewModel(), MnemonicRevealContract {
    override val mnemonic = flowOf { interactor.getMnemonic().toWordList().toImmutableList() }
        .stateInBackground(SharingStarted.Eagerly, persistentListOf())

    override val isMnemonicHidden = MutableStateFlow(true)

    override fun back() {
        router.back()
    }

    override fun revealMnemonic() {
        isMnemonicHidden.value = false
    }
}
