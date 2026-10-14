package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model

import androidx.annotation.Keep

@Keep
data class RefreshTokenRequest(
    val refreshToken: String
)
