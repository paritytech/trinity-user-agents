package io.paritytech.polkadotapp.feature_backup_impl.recover.compose

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_backup_impl.recover.RecoverOptionsContract
import io.paritytech.polkadotapp.feature_backup_impl.recover.RecoverOptionsTestTags
import io.paritytech.polkadotapp.feature_backup_impl.recover.compose.components.OptionButton
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun RecoverOptionsScreen(contract: RecoverOptionsContract) {
    val isRecovering = contract.isRecovering.collectAsStateWithLifecycle().value

    BackHandler(enabled = isRecovering) {
        // no-op
    }

    RecoverOptionsScreenInternal(
        isRecovering = isRecovering,
        onRecoverFromBackup = contract::onRecoverFromBackup,
        onImportRecoveryPhrase = contract::onImportRecoveryPhrase,
        onDismiss = contract::onDismiss
    )
}

@Composable
private fun RecoverOptionsScreenInternal(
    isRecovering: Boolean,
    onRecoverFromBackup: () -> Unit,
    onImportRecoveryPhrase: () -> Unit,
    onDismiss: () -> Unit
) {
    NovaBottomSheetSurface {
        AnimatedContent(
            targetState = isRecovering,
            label = "RecoverOptionsLoading"
        ) { loading ->
            if (loading) {
                LoadingContent()
            } else {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .verticalScroll(rememberScrollState())
                        .padding(
                            vertical = PolkadotTheme.spacings.large,
                            horizontal = PolkadotTheme.spacings.mediumIncreased
                        )
                ) {
                    OptionButton(
                        title = stringResource(RCommon.string.create_or_recover_options_recover_google_drive_action_title),
                        description = stringResource(RCommon.string.create_or_recover_options_recover_google_drive_action_description),
                        onAction = onRecoverFromBackup
                    )

                    VerticalSpacer { small }

                    OptionButton(
                        modifier = Modifier.testTag(RecoverOptionsTestTags.IMPORT_RECOVERY_PHRASE),
                        title = stringResource(RCommon.string.create_or_recover_options_recover_phrase_action_title),
                        description = stringResource(RCommon.string.create_or_recover_options_recover_phrase_action_description),
                        onAction = onImportRecoveryPhrase
                    )

                    VerticalSpacer { mediumIncreased }

                    PolkadotTextButton(
                        modifier = Modifier.fillMaxWidth(),
                        text = stringResource(RCommon.string.common_close),
                        style = PolkadotButtonStyle.ghost(),
                        enabled = true,
                        onClick = onDismiss
                    )
                }
            }
        }
    }
}

private val LOADING_CONTENT_MIN_HEIGHT = 350.dp

@Composable
private fun LoadingContent() {
    LoadingScreenState(
        modifier = Modifier
            .fillMaxWidth()
            .height(LOADING_CONTENT_MIN_HEIGHT)
            .padding(PolkadotTheme.spacings.extraLarge),
    )
}
