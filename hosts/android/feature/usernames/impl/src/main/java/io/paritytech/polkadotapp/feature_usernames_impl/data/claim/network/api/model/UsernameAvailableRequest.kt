package io.paritytech.polkadotapp.feature_usernames_impl.data.claim.network.api.model

import androidx.annotation.Keep

@Keep
data class UsernameAvailableRequest(
    val usernames: List<String>
)
