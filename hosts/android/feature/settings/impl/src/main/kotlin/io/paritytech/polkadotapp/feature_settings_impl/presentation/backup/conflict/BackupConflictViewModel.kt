package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupConflictPayload
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.BackupConflictInteractor
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.models.BackupConflictStep
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject

@HiltViewModel
class BackupConflictViewModel @Inject constructor(
    private val router: SettingsRouter,
    private val interactor: BackupConflictInteractor,
    savedStateHandle: SavedStateHandle
) : BaseViewModel(), BackupConflictContract {
    private val payload: BackupConflictPayload = savedStateHandle.getPayload()

    override val step = MutableStateFlow<BackupConflictStep>(BackupConflictStep.Conflict(payload.backupCreatedAt))

    override fun proceedToOverride() {
        step.value = BackupConflictStep.Override()
    }

    override fun confirmOverride() = launchUnit {
        step.value = BackupConflictStep.Override(inProgress = true)

        interactor
            .overrideBackup()
            .onSuccess {
                router.backWithResult(BackupConflictBottomSheet.REQUEST_KEY, true)
            }
            .onFailure {
                step.value = BackupConflictStep.Override(inProgress = false)
                showPresentationError(UnexpectedPresentationError(it))
            }
    }

    override fun cancelOverride() {
        router.back()
    }
}
