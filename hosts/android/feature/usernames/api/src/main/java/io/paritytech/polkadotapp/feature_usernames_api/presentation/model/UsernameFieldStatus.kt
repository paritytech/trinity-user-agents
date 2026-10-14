package io.paritytech.polkadotapp.feature_usernames_api.presentation.model

import androidx.compose.runtime.Immutable

enum class UsernameFieldStyle { Neutral, Error, Success }

/**
 * What the username field renders beneath itself. Deliberately carries no semantics: the claim
 * and upgrade screens fail for unrelated reasons, so each owns its own state model and resolves
 * the text before handing it over.
 */
@Immutable
data class UsernameFieldStatus(
    val style: UsernameFieldStyle,
    /** `null` hides the status pill entirely. */
    val text: String?,
)
