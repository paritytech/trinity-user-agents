package io.paritytech.polkadotapp.tools_auth_impl.google.executors

import android.content.Intent
import androidx.activity.result.ActivityResult
import com.google.android.gms.auth.api.signin.GoogleSignIn
import com.google.android.gms.auth.api.signin.GoogleSignInAccount
import com.google.android.gms.auth.api.signin.GoogleSignInOptions
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.Scope
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.ActivityResultExecutor
import io.paritytech.polkadotapp.tools_auth_impl.BuildConfig

class GoogleSignInExecutor(
    private val contextManager: ContextManager,
    private val scope: String?,
) : ActivityResultExecutor<GoogleSignInAccount>(contextManager.requireActivity()) {
    override fun createIntent(): Intent {
        val signInOptionsBuilder = GoogleSignInOptions.Builder(GoogleSignInOptions.DEFAULT_SIGN_IN)
        with(signInOptionsBuilder) {
            requestEmail()
            requestIdToken(BuildConfig.GOOGLE_OAUTH_ID)

            if (scope != null) requestScopes(Scope(scope))
        }

        val googleSignInClient =
            GoogleSignIn.getClient(contextManager.requireActivity(), signInOptionsBuilder.build())

        return googleSignInClient.signInIntent
    }

    override fun handleResult(result: ActivityResult): Result<GoogleSignInAccount> {
        val task = GoogleSignIn.getSignedInAccountFromIntent(result.data)
        return runCatching { task.getResult(ApiException::class.java) }
    }
}
