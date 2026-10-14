package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.design.components.mnemonic.model.Word
import io.paritytech.polkadotapp.design.components.mnemonic.model.toWordList
import io.paritytech.polkadotapp.feature_backup_api.mnemonic.model.ConfirmMnemonicPayload
import io.paritytech.polkadotapp.feature_backup_api.mnemonic.model.parcel.fromParcelable
import io.paritytech.polkadotapp.feature_backup_impl.BackupRouter
import io.paritytech.polkadotapp.feature_backup_impl.ManualMnemonicInteractor
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.models.ConfirmationState
import io.paritytech.polkadotapp.feature_backup_impl.presentation.error.WrongMnemonicPresentationError
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class ConfirmMnemonicViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val router: BackupRouter,
    private val interactor: ManualMnemonicInteractor
) : BaseViewModel(), ConfirmMnemonicContract {
    private val payload: ConfirmMnemonicPayload = savedStateHandle.getPayload()

    private val generatedMnemonic = payload.mnemonic.fromParcelable()

    override val confirmationState = MutableStateFlow(
        ConfirmationState(
            allWords = generatedMnemonic.wordList.shuffled().toWordList().toImmutableList()
        )
    )
    override val inProgress = MutableStateFlow(false)

    override fun back() {
        router.back()
    }

    override fun add(word: Word) {
        if (inProgress.value) return

        confirmationState.update { currentState ->
            currentState.copy(
                addedWords = (currentState.addedWords + word).toImmutableList()
            )
        }
    }

    override fun remove(word: Word) {
        if (inProgress.value) return

        confirmationState.update { currentState ->
            currentState.copy(
                addedWords = (currentState.addedWords - word).toImmutableList()
            )
        }
    }

    override fun proceed() {
        val enteredCorrectly = confirmationState.value.addedWords == generatedMnemonic.wordList

        if (enteredCorrectly.not()) {
            showPresentationError(WrongMnemonicPresentationError())
            return
        }

        inProgress.enable()

        launch {
            interactor.createAccounts(generatedMnemonic.entropy)
                .onSuccess {
                    // navigation is handled in RootInteractor
                }
                .onFailure {
                    inProgress.disable()
                    showPresentationError(UnexpectedPresentationError(it))
                }
        }
    }
}
