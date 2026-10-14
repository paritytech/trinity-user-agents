package io.paritytech.polkadotapp.feature_usernames_api.presentation

import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

const val MIN_USERNAME_LENGTH = 6
const val MAX_USERNAME_LENGTH = 32

fun String.filterAvailableUsernameSymbols(): String {
    return filter { it.isLetterOrDigit() || it == Username.SEPARATOR }
}

fun String.filterUsernameInput(): String = filter { it.isLetter() }
    .take(MAX_USERNAME_LENGTH)
    .lowercase()
