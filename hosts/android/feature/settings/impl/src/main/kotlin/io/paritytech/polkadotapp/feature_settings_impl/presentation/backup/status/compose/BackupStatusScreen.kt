@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.BackupStatusContract
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.compose.components.*
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.models.BackupStatusUiState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun BackupStatusScreen(contract: BackupStatusContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    var isRecoveryPhraseConfirmationVisible by remember { mutableStateOf(false) }

    BackupStatusScreenInternal(
        state = state,
        onBackAction = contract::back,
        onAllowGoogleDrive = contract::onAllowGoogleDrive,
        onDeclineGoogleDrive = contract::onDeclineGoogleDrive,
        onBackupAction = contract::onCreateBackup,
        onSecretPhraseAction = { isRecoveryPhraseConfirmationVisible = true },
        onOverrideBackupAction = contract::onOverrideBackup
    )

    NovaModalBottomSheet(
        isVisible = isRecoveryPhraseConfirmationVisible,
        onDismissRequest = { isRecoveryPhraseConfirmationVisible = false }
    ) {
        MnemonicConfirmationContent(
            onConfirm = {
                isRecoveryPhraseConfirmationVisible = false
                contract.onShowMnemonic()
            },
            onCancel = { isRecoveryPhraseConfirmationVisible = false }
        )
    }
}

@Composable
private fun BackupStatusScreenInternal(
    state: BackupStatusUiState,
    onBackAction: () -> Unit,
    onAllowGoogleDrive: () -> Unit,
    onDeclineGoogleDrive: () -> Unit,
    onBackupAction: () -> Unit,
    onSecretPhraseAction: () -> Unit,
    onOverrideBackupAction: () -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .systemBarsPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.settings_backup_title),
                navigationAction = rememberTopBarAction(onBackAction),
                titleAlignment = TopBarTitleAlignment.Center
            )

            when (state) {
                BackupStatusUiState.NoAccess -> {
                    GoogleDrivePermissionContent(
                        onAllowAction = onAllowGoogleDrive,
                        onDeclineAction = onDeclineGoogleDrive
                    )
                }

                BackupStatusUiState.BackupExists -> {
                    BackupExistsContent(
                        onSecretPhraseAction = onSecretPhraseAction
                    )
                }

                BackupStatusUiState.BackupInProgress -> {
                    BackupInProgressContent(
                        onSecretPhraseAction = onSecretPhraseAction
                    )
                }

                BackupStatusUiState.NoBackup -> {
                    NoBackupContent(
                        onBackupAction = onBackupAction,
                        onSecretPhraseAction = onSecretPhraseAction
                    )
                }

                BackupStatusUiState.CheckingForBackup -> {
                    CheckingForBackup()
                }

                is BackupStatusUiState.BackupConflict -> {
                    BackupConflictContent(
                        onOverrideAction = onOverrideBackupAction,
                        onSecretPhraseAction = onSecretPhraseAction
                    )
                }
            }
        }
    }
}

@Preview
@Composable
private fun BackupStatusScreenPreview() {
    PolkadotTheme {
        BackupStatusScreenInternal(
            state = BackupStatusUiState.NoBackup,
            onBackAction = {},
            onAllowGoogleDrive = {},
            onDeclineGoogleDrive = {},
            onBackupAction = {},
            onSecretPhraseAction = {},
            onOverrideBackupAction = {}
        )
    }
}
