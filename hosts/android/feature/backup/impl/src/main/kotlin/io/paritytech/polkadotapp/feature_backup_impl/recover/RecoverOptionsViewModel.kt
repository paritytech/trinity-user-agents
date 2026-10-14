package io.paritytech.polkadotapp.feature_backup_impl.recover

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_backup_api.presentation.RecoverOptionsPayload
import io.paritytech.polkadotapp.feature_backup_impl.BackupRouter
import io.paritytech.polkadotapp.feature_backup_impl.presentation.error.toImportFromBackupPresentationError
import io.paritytech.polkadotapp.feature_backup_impl.recover.domain.RecoverOptionsInteractor
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject

@HiltViewModel
class RecoverOptionsViewModel @Inject constructor(
    private val router: BackupRouter,
    private val interactor: RecoverOptionsInteractor
) : BaseViewModel(), RecoverOptionsContract {
    override val isRecovering = MutableStateFlow(false)

    override fun onRecoverFromBackup() = launchUnit {
        if (isRecovering.value) return@launchUnit

        isRecovering.enable()

        interactor.importAccountFromBackup()
            .onSuccess {
                router.backWithResult(RecoverOptionsPayload.REQUEST_KEY, RecoverOptionsPayload.Result.IMPORTED_FROM_BACKUP)
            }
            .onFailure { t ->
                isRecovering.disable()

                showPresentationError(t.toImportFromBackupPresentationError())
            }
    }

    override fun onImportRecoveryPhrase() {
        if (isRecovering.value) return
        router.openRecoverMnemonic()
    }

    override fun onDismiss() {
        router.back()
    }
}
