package io.paritytech.polkadotapp.tools_authentication_impl.data

interface AuthenticationMethod {
    suspend fun authenticateUser(): Result<Unit>
}
