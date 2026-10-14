package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.compose

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDefaults
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.BackupConflictContract
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.compose.components.ConflictStep
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.compose.components.OverrideStep
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.models.BackupConflictStep

@Composable
fun BackupConflictScreen(contract: BackupConflictContract) {
    val step by contract.step.collectAsStateWithLifecycle()

    BackHandler {
        contract.cancelOverride()
    }

    BackupConflictScreenInternal(
        step = step,
        onProceedToOverride = contract::proceedToOverride,
        onConfirmOverride = contract::confirmOverride,
        onCancelAction = contract::cancelOverride,
    )
}

@Composable
private fun BackupConflictScreenInternal(
    step: BackupConflictStep,
    onProceedToOverride: () -> Unit,
    onConfirmOverride: () -> Unit,
    onCancelAction: () -> Unit
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
            contentKey = { it::class },
            transitionSpec = { NovaBottomSheetDefaults.PAGE_TRANSITION_SPEC },
            label = "content"
        ) {
            when (it) {
                is BackupConflictStep.Conflict -> ConflictStep(
                    step = it,
                    onOverrideAction = onProceedToOverride,
                    onCancelAction = onCancelAction,
                )

                is BackupConflictStep.Override -> OverrideStep(
                    step = it,
                    onOverrideAction = onConfirmOverride,
                    onCancelAction = onCancelAction,
                )
            }
        }
    }
}
