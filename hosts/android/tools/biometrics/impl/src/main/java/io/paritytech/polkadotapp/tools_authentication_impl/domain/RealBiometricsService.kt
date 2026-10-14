package io.paritytech.polkadotapp.tools_authentication_impl.domain

import io.paritytech.polkadotapp.tools_authentication_api.data.AuthenticationSession
import io.paritytech.polkadotapp.tools_authentication_api.domain.BiometricsService
import io.paritytech.polkadotapp.tools_authentication_impl.data.AuthenticationMethod
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject

class RealBiometricsService @Inject constructor(
    private val authMethod: AuthenticationMethod,
    private val authenticationSession: AuthenticationSession,
) : BiometricsService {
    private val authMutex = Mutex()

    override suspend fun performSessionAuthentication() = authMutex.withLock {
        if (authenticationSession.isAuthenticated()) return Result.success(Unit)

        authMethod.authenticateUser().onSuccess {
            authenticationSession.markAuthenticated()
        }
    }

    override suspend fun performOneTimeAuthentication() = authMutex.withLock {
        authMethod.authenticateUser()
    }
}
