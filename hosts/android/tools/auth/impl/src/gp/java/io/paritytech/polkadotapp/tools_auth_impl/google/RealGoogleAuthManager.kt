package io.paritytech.polkadotapp.tools_auth_impl.google

import com.google.android.gms.auth.UserRecoverableAuthException
import com.google.android.gms.auth.api.signin.GoogleSignIn
import com.google.android.gms.auth.api.signin.GoogleSignInOptions
import com.google.api.client.googleapis.extensions.android.gms.auth.UserRecoverableAuthIOException
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.tools_auth_api.GoogleAuthManager
import io.paritytech.polkadotapp.tools_auth_impl.google.executors.GoogleSignInExecutor
import io.paritytech.polkadotapp.tools_auth_impl.google.executors.RemoteConsentExecutor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.tasks.await
import kotlinx.coroutines.withContext
import javax.inject.Inject

class RealGoogleAuthManager @Inject constructor(
    private val contextManager: ContextManager,
) : GoogleAuthManager {
    override suspend fun isAuthorized(): Boolean = withContext(Dispatchers.IO) {
        GoogleSignIn.getLastSignedInAccount(contextManager.applicationContext) != null
    }

    override suspend fun signOut() {
        withContext(Dispatchers.IO) {
            val options = GoogleSignInOptions.Builder(GoogleSignInOptions.DEFAULT_SIGN_IN).build()
            val client = GoogleSignIn.getClient(contextManager.applicationContext, options)
            client.signOut().await()
        }
    }

    override suspend fun <T> runAuthenticated(
        scope: String?,
        action: suspend (String?) -> Result<T>
    ): Result<T> {
        return GoogleSignInExecutor(contextManager, scope).execute()
            .mapCatching { it.idToken }
            .flatMap { runCatchingRecoveringAuthErrors(it, action) }
    }

    private suspend fun <T> runCatchingRecoveringAuthErrors(
        token: String?,
        action: suspend (String?) -> Result<T>
    ): Result<T> {
        return action(token)
            .recoverCatching {
                when (it) {
                    is UserRecoverableAuthException -> it.askForConsent()
                    is UserRecoverableAuthIOException -> it.cause?.askForConsent()
                    else -> throw it
                }

                action(token).getOrThrow()
            }
    }

    private suspend fun UserRecoverableAuthException.askForConsent(): Result<Unit> {
        return RemoteConsentExecutor(contextManager, this).execute()
    }
}
