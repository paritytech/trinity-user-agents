package io.paritytech.polkadotapp.tools_auth_impl.google.executors

import android.app.Activity.RESULT_OK
import android.content.Intent
import androidx.activity.result.ActivityResult
import com.google.android.gms.auth.UserRecoverableAuthException
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.ActivityResultExecutor

class RemoteConsentExecutor(
    contextManager: ContextManager,
    private val consentException: UserRecoverableAuthException,
) : ActivityResultExecutor<Unit>(contextManager.requireActivity()) {
    override fun createIntent(): Intent = consentException.intent!!

    override fun handleResult(result: ActivityResult): Result<Unit> {
        return if (result.resultCode == RESULT_OK) {
            Result.success(Unit)
        } else {
            Result.failure(consentException)
        }
    }
}
