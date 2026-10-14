package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.recover

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.emit
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_backup_impl.BackupRouter
import io.paritytech.polkadotapp.feature_backup_impl.ManualMnemonicInteractor
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject

@HiltViewModel
class RecoverMnemonicViewModel @Inject constructor(
    private val router: BackupRouter,
    private val interactor: ManualMnemonicInteractor
) : BaseViewModel(), RecoverMnemonicContract {
    override val enteredMnemonic = MutableStateFlow("")
    override val inProgress = MutableStateFlow(false)

    override val invalidMnemonicEvents = MutableSharedFlow<Unit>()

    override fun back() {
        router.back()
    }

    override fun enterMnemonic(mnemonic: String) {
        enteredMnemonic.value = mnemonic
    }

    override fun recover() = launchUnit {
        inProgress.enable()

        interactor.restoreAccountWithEnteredMnemonic(enteredMnemonic.value)
            .onSuccess {
                router.back()
            }
            .onFailure {
                invalidMnemonicEvents.emit()
                inProgress.disable()
            }
    }
}
