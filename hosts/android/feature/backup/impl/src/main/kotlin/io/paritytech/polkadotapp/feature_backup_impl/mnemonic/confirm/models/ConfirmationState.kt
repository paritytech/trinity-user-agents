package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.design.components.mnemonic.model.Word
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

@Immutable
data class ConfirmationState(
    val addedWords: ImmutableList<Word> = persistentListOf(),
    val allWords: ImmutableList<Word> = persistentListOf()
)
