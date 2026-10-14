package io.paritytech.polkadotapp.tools_authentication_impl.data

import io.paritytech.polkadotapp.tools_authentication_api.data.AuthenticationSession
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject

class InMemoryAuthenticationSession @Inject constructor() : AuthenticationSession {
    override val hasAuthenticatedInCurrentSession = MutableStateFlow(false)

    override fun isAuthenticated(): Boolean {
        return hasAuthenticatedInCurrentSession.value
    }

    override fun markAuthenticated() {
        hasAuthenticatedInCurrentSession.value = true
    }
}
