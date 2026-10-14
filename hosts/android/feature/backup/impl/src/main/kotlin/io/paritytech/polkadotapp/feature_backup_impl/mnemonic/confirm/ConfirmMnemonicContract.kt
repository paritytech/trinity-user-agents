package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm

import io.paritytech.polkadotapp.design.components.mnemonic.model.Word
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.models.ConfirmationState
import kotlinx.coroutines.flow.StateFlow

interface ConfirmMnemonicContract {
    val confirmationState: StateFlow<ConfirmationState>
    val inProgress: StateFlow<Boolean>
    fun back()
    fun add(word: Word)
    fun remove(word: Word)
    fun proceed()
}
