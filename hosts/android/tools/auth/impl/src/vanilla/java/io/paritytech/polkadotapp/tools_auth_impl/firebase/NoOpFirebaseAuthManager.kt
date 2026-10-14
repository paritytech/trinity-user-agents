package io.paritytech.polkadotapp.tools_auth_impl.firebase

import io.paritytech.polkadotapp.tools_auth_api.FirebaseAuthManager
import javax.inject.Inject

class NoOpFirebaseAuthManager @Inject constructor() : FirebaseAuthManager {
    override suspend fun authenticate(scope: String?): Result<String> {
        return Result.failure(UnsupportedOperationException("Firebase auth is not available in vanilla build"))
    }
}
