package io.paritytech.polkadotapp.tools_auth_api

interface FirebaseAuthManager {
    suspend fun authenticate(scope: String?): Result<String>
}
