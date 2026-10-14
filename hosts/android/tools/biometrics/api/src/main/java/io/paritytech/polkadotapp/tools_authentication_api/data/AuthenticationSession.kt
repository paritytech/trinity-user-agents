package io.paritytech.polkadotapp.tools_authentication_api.data

import kotlinx.coroutines.flow.Flow

interface AuthenticationSession {
    val hasAuthenticatedInCurrentSession: Flow<Boolean>

    fun isAuthenticated(): Boolean

    fun markAuthenticated()
}
