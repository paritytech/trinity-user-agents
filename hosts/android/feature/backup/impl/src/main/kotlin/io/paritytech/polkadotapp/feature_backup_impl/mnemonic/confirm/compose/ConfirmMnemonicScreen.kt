package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.compose

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.mnemonic.model.Word
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.common.compose.components.MnemonicBase
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.ConfirmMnemonicContract
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.compose.components.MnemonicConfirmation
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.models.ConfirmationState
import kotlinx.collections.immutable.toImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ConfirmMnemonicScreen(contract: ConfirmMnemonicContract) {
    ConfirmMnemonicScreenInternal(
        onBackAction = contract::back,
        confirmationState = contract.confirmationState.collectAsStateWithLifecycle().value,
        onAdd = contract::add,
        onRemove = contract::remove,
        onContinueAction = contract::proceed,
        inProgress = contract.inProgress.collectAsStateWithLifecycle().value
    )
}

@Composable
private fun ConfirmMnemonicScreenInternal(
    onBackAction: () -> Unit,
    confirmationState: ConfirmationState,
    onContinueAction: () -> Unit,
    onAdd: (Word) -> Unit,
    onRemove: (Word) -> Unit,
    inProgress: Boolean
) {
    Column(modifier = Modifier.fillMaxSize()) {
        MnemonicBase(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .systemBarsPadding(),
            onBackAction = onBackAction,
            title = stringResource(RCommon.string.manual_backup_confirm_title),
            description = stringResource(RCommon.string.manual_backup_confirm_description)
        ) {
            MnemonicConfirmation(
                confirmationState = confirmationState,
                onAdd = onAdd,
                onRemove = onRemove
            )
        }

        Box(
            modifier = Modifier
                .fillMaxWidth()
                .systemBarsPadding()
                .padding(PolkadotTheme.spacings.large)
        ) {
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.common_continue),
                onClick = onContinueAction,
                loading = inProgress
            )
        }
    }
}

@Preview
@Composable
private fun ConfirmMnemonicScreenPreview() {
    PolkadotTheme {
        ConfirmMnemonicScreenInternal(
            onBackAction = {},
            confirmationState = ConfirmationState(
                addedWords = List(3) { Word(it, "word$it") }.toImmutableList(),
                allWords = List(12) { Word(it, "word$it") }.toImmutableList(),
            ),
            onContinueAction = {},
            onAdd = {},
            onRemove = {},
            inProgress = false
        )
    }
}
