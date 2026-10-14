package io.paritytech.polkadotapp.feature_backup_impl.backupFound

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupFoundPayload
import io.paritytech.polkadotapp.feature_backup_impl.BackupRouter
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.domain.BackupFoundInteractor
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.models.BackupFoundProgressState
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.models.BackupFoundStep
import io.paritytech.polkadotapp.feature_backup_impl.presentation.error.toImportFromBackupPresentationError
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject

@HiltViewModel
class BackupFoundViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val router: BackupRouter,
    private val interactor: BackupFoundInteractor
) : BaseViewModel(), BackupFoundContract {
    private val payload: BackupFoundPayload = savedStateHandle.getPayload()
    private val accountId = payload.accountId.intoAccountId()

    override val progressState = MutableStateFlow(BackupFoundProgressState.IDLE)

    override val step =
        MutableStateFlow<BackupFoundStep>(BackupFoundStep.Recover(payload.createdAt))

    override val username = MutableStateFlow<LoadingState<String?>>(LoadingState.Loading)

    init {
        loadUsername()
    }

    private fun loadUsername() = launchUnit {
        interactor.getUsername(accountId)
            .onSuccess { username.value = LoadingState.Loaded(it) }
            .onFailure { username.value = LoadingState.Loaded(null) }
    }

    override fun backupOverrideIntention() {
        step.value = BackupFoundStep.Override
    }

    override fun backupOverrideConfirm() = launchUnit {
        progressState.value = BackupFoundProgressState.OVERRIDING

        interactor.createAccountsAndOverrideBackup()
            .onSuccess {
                router.backWithResult(BackupFoundPayload.REQUEST_KEY, BackupFoundPayload.Result.OVERRIDDEN)
            }
            .onFailure {
                progressState.value = BackupFoundProgressState.IDLE
                showPresentationError(UnexpectedPresentationError(it))
            }
    }

    override fun backupOverrideCancel() {
        step.value = BackupFoundStep.Recover(payload.createdAt)
    }

    override fun recoverBackup() = launchUnit {
        progressState.value = BackupFoundProgressState.RECOVERING

        interactor.importAccountFromBackup()
            .onSuccess {
                router.backWithResult(BackupFoundPayload.REQUEST_KEY, BackupFoundPayload.Result.RECOVERED)
            }
            .onFailure {
                progressState.value = BackupFoundProgressState.IDLE
                showPresentationError(it.toImportFromBackupPresentationError())
            }
    }
}
