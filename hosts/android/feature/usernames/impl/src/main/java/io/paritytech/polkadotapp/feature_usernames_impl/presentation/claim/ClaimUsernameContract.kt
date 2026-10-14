package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_usernames_api.presentation.model.DigitsFieldState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow

enum class ClaimUsernameProgress {
    NONE,
    CLAIMING,
    CREATING,
    RECOVERING
}

@Immutable
data class ClaimUsernameState(
    val username: String = "",
    /** Zero-padded digit suffixes returned by the backend for client-side validation. */
    val availableDigits: ImmutableList<String> = persistentListOf(),
    val fieldState: ClaimUsernameFieldState = ClaimUsernameFieldState.Neutral,
    val digitsFieldState: DigitsFieldState = DigitsFieldState.Hidden,
    val progress: ClaimUsernameProgress = ClaimUsernameProgress.NONE,
    val showRecoverOption: Boolean = true,
) {
    val claimButtonEnabled: Boolean
        get() = fieldState is ClaimUsernameFieldState.Available &&
            (digitsFieldState is DigitsFieldState.Hidden || (digitsFieldState is DigitsFieldState.Visible && digitsFieldState.isValid))

    val showClearAction: Boolean
        get() = fieldState is ClaimUsernameFieldState.Taken
}

interface ClaimUsernameContract {
    val state: StateFlow<ClaimUsernameState>

    val messageEvents: SharedFlow<Int>

    fun backPressed()

    fun onUsernameChanged(value: String)

    fun onDigitsChanged(value: String)

    fun onClaimClicked()

    fun onClearAction()

    fun onRecoverClicked()

    fun onTermsClicked()

    fun onPrivacyPolicyClicked()

    fun onBackupOverridden()

    fun onImportedFromBackup()
}
