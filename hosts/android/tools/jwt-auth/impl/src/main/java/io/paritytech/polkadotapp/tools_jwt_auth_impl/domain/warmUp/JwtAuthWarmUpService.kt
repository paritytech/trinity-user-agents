package io.paritytech.polkadotapp.tools_jwt_auth_impl.domain.warmUp

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.manager.JWTTokenProvider
import javax.inject.Inject

interface JwtAuthWarmUpService {
    suspend fun warmUpToken()
}

class RealJwtAuthWarmUpService @Inject internal constructor(
    private val tokenProvider: JWTTokenProvider
) : JwtAuthWarmUpService {
    override suspend fun warmUpToken() {
        runCatching { tokenProvider.validToken() }
            .logFailure("Failed to warm up bearer token")
    }
}
