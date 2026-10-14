package io.paritytech.polkadotapp.feature_backup_impl.backupFound.compose

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDefaults
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.BackupFoundContract
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.compose.components.OverrideStep
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.compose.components.RecoverStep
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.models.BackupFoundProgressState
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.models.BackupFoundStep

@Composable
fun BackupFoundScreen(contract: BackupFoundContract) {
    BackupFoundScreenInternal(
        progressState = contract.progressState.collectAsStateWithLifecycle().value,
        step = contract.step.collectAsStateWithLifecycle().value,
        username = contract.username.collectAsStateWithLifecycle().value,
        onInitializeOverride = contract::backupOverrideIntention,
        onRecover = contract::recoverBackup,
        onConfirmOverride = contract::backupOverrideConfirm,
        onCancelOverride = contract::backupOverrideCancel
    )
}

@Composable
private fun BackupFoundScreenInternal(
    progressState: BackupFoundProgressState,
    step: BackupFoundStep,
    username: LoadingState<String?>,
    onInitializeOverride: () -> Unit,
    onRecover: () -> Unit,
    onConfirmOverride: () -> Unit,
    onCancelOverride: () -> Unit
) {
    PolkadotSurface(
        modifier = Modifier
            .systemBarsPadding()
            .padding(PolkadotTheme.spacings.small),
        color = PolkadotTheme.colors.bg.surface.nested,
        shape = PolkadotTheme.shapes.extraLarge
    ) {
        AnimatedContent(
            targetState = step,
            transitionSpec = { NovaBottomSheetDefaults.PAGE_TRANSITION_SPEC },
            label = "content"
        ) {
            when (it) {
                is BackupFoundStep.Recover -> RecoverStep(
                    step = it,
                    username = username,
                    onRecoverAction = onRecover,
                    onOverrideAction = onInitializeOverride,
                    progressState = progressState
                )

                is BackupFoundStep.Override -> OverrideStep(
                    onOverrideAction = onConfirmOverride,
                    onOverrideCancel = onCancelOverride,
                    progressState = progressState
                )
            }
        }
    }
}
