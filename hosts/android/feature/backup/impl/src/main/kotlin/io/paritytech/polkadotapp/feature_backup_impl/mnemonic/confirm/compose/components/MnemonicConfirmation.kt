package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.compose.components

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.mnemonic.Mnemonic
import io.paritytech.polkadotapp.design.components.mnemonic.MnemonicWord
import io.paritytech.polkadotapp.design.components.mnemonic.model.Word
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.models.ConfirmationState
import kotlinx.collections.immutable.toImmutableList

@Composable
fun MnemonicConfirmation(
    confirmationState: ConfirmationState,
    onAdd: (Word) -> Unit,
    onRemove: (Word) -> Unit
) {
    Column(
        modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
    ) {
        Mnemonic(
            mnemonic = confirmationState.addedWords,
            onWordClickAction = onRemove
        )

        VerticalSpacer { mediumIncreased }

        LazyVerticalGrid(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = LocalConfiguration.current.screenHeightDp.dp)
                .padding(horizontal = PolkadotTheme.spacings.small),
            columns = GridCells.Fixed(3),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium)
        ) {
            items(confirmationState.allWords) {
                val isWordAdded = confirmationState.addedWords.contains(it)
                MnemonicWord(
                    modifier = Modifier
                        .alpha(animateFloatAsState(if (isWordAdded) 0f else 1f).value),
                    word = it.value,
                    onClick = { onAdd(it) },
                    enabled = isWordAdded.not()
                )
            }
        }
    }
}

@Preview
@Composable
private fun MnemonicConfirmationPreview() {
    PolkadotTheme {
        MnemonicConfirmation(
            confirmationState = ConfirmationState(
                addedWords = List(3) { Word(it, "word$it") }.toImmutableList(),
                allWords = List(12) { Word(it, "word$it") }.toImmutableList(),
            ),
            onAdd = {},
            onRemove = {}
        )
    }
}
