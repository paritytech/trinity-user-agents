package io.paritytech.polkadotapp.tools_authentication_impl.data.methods

import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import io.paritytech.polkadotapp.common.presentation.navigation.OverlayCoordinator
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.tools_authentication_api.domain.AuthenticationCancelledException
import io.paritytech.polkadotapp.tools_authentication_impl.data.AuthenticationMethod
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import javax.inject.Inject
import kotlin.coroutines.resume
import io.paritytech.polkadotapp.common.R as RCommon

class BiometricPromptAuthenticationMethod @Inject constructor(
    private val contextManager: ContextManager,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val overlayCoordinator: OverlayCoordinator,
) : AuthenticationMethod {
    companion object {
        private const val ALLOWED_BIOMETRICS = BIOMETRIC_WEAK or DEVICE_CREDENTIAL
    }

    private val executor = ContextCompat.getMainExecutor(contextManager.requireActivity())

    override suspend fun authenticateUser(): Result<Unit> {
        return overlayCoordinator.forbidOverlays().use {
            authenticateUseInternal()
        }
    }

    private suspend fun authenticateUseInternal(): Result<Unit> {
        val biometricManager = BiometricManager.from(contextManager.requireActivity())

        val context = contextManager.applicationContext

        return biometricManager.prepareForAuthentication().flatMap {
            val promptInfo = BiometricPrompt.PromptInfo.Builder()
                .setTitle(context.getString(RCommon.string.authentication_title))
                .setDescription(context.getString(RCommon.string.authentication_description))
                .setAllowedAuthenticators(ALLOWED_BIOMETRICS)
                .build()

            promptInfo.authenticate()
        }
    }

    private suspend fun BiometricPrompt.PromptInfo.authenticate(): Result<Unit> {
        return withContext(coroutineDispatchers.main) {
            suspendCancellableCoroutine { continuation ->
                val biometricPrompt = BiometricPrompt(
                    contextManager.requireActivity() as FragmentActivity, executor,
                    object : BiometricPrompt.AuthenticationCallback() {
                        override fun onAuthenticationError(
                            errorCode: Int,
                            errString: CharSequence,
                        ) {
                            val exception =
                                AuthenticationCancelledException("$errorCode: $errString")
                            continuation.resume(Result.failure(exception))
                        }

                        override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                            continuation.resume(Result.success(Unit))
                        }
                    })

                continuation.invokeOnCancellation {
                    biometricPrompt.cancelAuthentication()
                }

                biometricPrompt.authenticate(this@authenticate)
            }
        }
    }

    private suspend fun BiometricManager.prepareForAuthentication(): Result<Unit> {
        return when (canAuthenticate(ALLOWED_BIOMETRICS)) {
            BiometricManager.BIOMETRIC_SUCCESS -> Result.success(Unit)

            BiometricManager.BIOMETRIC_ERROR_NO_HARDWARE ->
                Result.failure(AuthenticationCancelledException("No matching hardware found to perform authentication"))

            BiometricManager.BIOMETRIC_ERROR_HW_UNAVAILABLE ->
                Result.failure(AuthenticationCancelledException("Hardware to perform authentication is currently unavailable. Try again later."))

            BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED -> {
                BiometricExecutor(contextManager).execute()
            }

            else -> Result.failure(AuthenticationCancelledException("Unknown authentication hardware state"))
        }
    }
}
