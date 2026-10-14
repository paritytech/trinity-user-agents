package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.mnemonic

import io.paritytech.polkadotapp.design.components.mnemonic.model.Word
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.flow.StateFlow

interface MnemonicRevealContract {
    val mnemonic: StateFlow<ImmutableList<Word>>
    val isMnemonicHidden: StateFlow<Boolean>

    fun back()
    fun revealMnemonic()
}
