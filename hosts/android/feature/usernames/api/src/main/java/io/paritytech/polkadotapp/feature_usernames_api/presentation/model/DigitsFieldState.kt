package io.paritytech.polkadotapp.feature_usernames_api.presentation.model

/** State of the digit suffix field on the claim username screen. */
sealed interface DigitsFieldState {
    /** Username is not yet available — digits field not shown. */
    data object Hidden : DigitsFieldState

    /** Digits field is visible with the entered [digits] value. */
    data class Visible(val digits: String, val isValid: Boolean) : DigitsFieldState
}
