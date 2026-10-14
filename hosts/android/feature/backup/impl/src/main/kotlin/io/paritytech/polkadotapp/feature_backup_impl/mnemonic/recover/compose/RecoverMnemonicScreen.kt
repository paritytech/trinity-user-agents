package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.recover.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.dialog.NovaAlertDialog
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.NovaTextField
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleSize
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.recover.RecoverMnemonicContract
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.recover.RecoverMnemonicTestTags
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun RecoverMnemonicScreen(contract: RecoverMnemonicContract) {
    var invalidMnemonicAlertVisible by remember { mutableStateOf(false) }
    contract.invalidMnemonicEvents.collectAsEffect { _, _ ->
        invalidMnemonicAlertVisible = true
    }

    if (invalidMnemonicAlertVisible) {
        NovaAlertDialog(
            title = stringResource(RCommon.string.recover_mnemonic_invalid_mnemonic_title),
            text = stringResource(RCommon.string.recover_mnemonic_invalid_mnemonic_description),
            positiveButtonTitle = stringResource(RCommon.string.recover_mnemonic_invalid_mnemonic_action),
            onPositiveButtonClick = { invalidMnemonicAlertVisible = false },
            onDismissRequest = { invalidMnemonicAlertVisible = false }
        )
    }

    RecoverMnemonicScreenInternal(
        onBackAction = contract::back,
        onMnemonicEnter = contract::enterMnemonic,
        enteredMnemonic = contract.enteredMnemonic.collectAsStateWithLifecycle().value,
        onRecoverAction = contract::recover,
        inProgress = contract.inProgress.collectAsStateWithLifecycle().value
    )
}

@Composable
private fun RecoverMnemonicScreenInternal(
    onBackAction: () -> Unit,
    enteredMnemonic: String,
    onMnemonicEnter: (String) -> Unit,
    onRecoverAction: () -> Unit,
    inProgress: Boolean
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .systemBarsPadding()
                .imePadding()
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
                    .verticalScroll(rememberScrollState())
            ) {
                PolkadotTopBar(
                    title = stringResource(RCommon.string.recover_mnemonic_title),
                    navigationAction = rememberTopBarAction(onBackAction),
                    titleSize = TopBarTitleSize.Large,
                )

                VerticalSpacer { small }

                Column(
                    modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.large)
                ) {
                    NovaText(
                        text = stringResource(RCommon.string.recover_mnemonic_description),
                        style = PolkadotTheme.typography.body.large,
                        color = PolkadotTheme.colors.fg.secondary
                    )

                    VerticalSpacer { mediumIncreased }

                    NovaTextField(
                        modifier = Modifier
                            .heightIn(min = 136.dp)
                            .fillMaxWidth()
                            .testTag(RecoverMnemonicTestTags.RECOVERY_PHRASE_INPUT),
                        value = enteredMnemonic,
                        onValueChange = onMnemonicEnter,
                        singleLine = false,
                        readOnly = inProgress
                    )
                }
            }

            PolkadotTextButton(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(PolkadotTheme.spacings.large)
                    .testTag(RecoverMnemonicTestTags.RECOVERY_PHRASE_SUBMIT_BUTTON),
                text = stringResource(RCommon.string.recover_mnemonic_action),
                enabled = enteredMnemonic.isNotEmpty(),
                onClick = onRecoverAction,
                loading = inProgress
            )
        }
    }
}

@Preview
@Composable
private fun RecoverMnemonicScreenPreview() {
    PolkadotTheme {
        RecoverMnemonicScreenInternal(
            onBackAction = {},
            enteredMnemonic = "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
            onMnemonicEnter = {},
            onRecoverAction = {},
            inProgress = false
        )
    }
}
