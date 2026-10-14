package io.paritytech.polkadotapp.tools_authentication_impl.data.methods

import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.activity.result.ActivityResult
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.ActivityResultExecutor
import io.paritytech.polkadotapp.tools_authentication_api.domain.AuthenticationCancelledException

class BiometricExecutor(
    private val contextManager: ContextManager,
) : ActivityResultExecutor<Unit>(contextManager.requireActivity()) {
    override fun createIntent(): Intent {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Intent(Settings.ACTION_BIOMETRIC_ENROLL).apply {
                putExtra(
                    Settings.EXTRA_BIOMETRIC_AUTHENTICATORS_ALLOWED,
                    BIOMETRIC_WEAK or DEVICE_CREDENTIAL
                )
            }
        } else {
            Intent(Settings.ACTION_SECURITY_SETTINGS)
        }
    }

    override fun handleResult(result: ActivityResult): Result<Unit> {
        val keyguardManager =
            contextManager.requireActivity()
                .getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager

        return if (keyguardManager.isDeviceSecure) {
            Result.success(Unit)
        } else {
            Result.failure(AuthenticationCancelledException("Cannot use the app without device lock set"))
        }
    }
}
