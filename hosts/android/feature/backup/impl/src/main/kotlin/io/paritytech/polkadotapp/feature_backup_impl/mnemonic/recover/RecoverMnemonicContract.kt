package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.recover

import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow

interface RecoverMnemonicContract {
    val enteredMnemonic: StateFlow<String>

    val inProgress: StateFlow<Boolean>

    val invalidMnemonicEvents: SharedFlow<Unit>

    fun back()

    fun enterMnemonic(mnemonic: String)

    fun recover()
}
