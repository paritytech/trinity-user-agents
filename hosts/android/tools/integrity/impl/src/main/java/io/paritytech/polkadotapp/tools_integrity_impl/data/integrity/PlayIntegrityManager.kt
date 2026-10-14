package io.paritytech.polkadotapp.tools_integrity_impl.data.integrity

import com.google.android.play.core.integrity.IntegrityManager
import com.google.android.play.core.integrity.IntegrityTokenRequest
import io.paritytech.polkadotapp.tools_integrity_impl.BuildConfig
import kotlinx.coroutines.suspendCancellableCoroutine
import javax.inject.Inject

interface PlayIntegrityManager {
    suspend fun generateIntegrityToken(nonce: String): Result<String>
}

class RealPlayIntegrityManager @Inject constructor(
    private val integrityManager: IntegrityManager,
) : PlayIntegrityManager {
    override suspend fun generateIntegrityToken(nonce: String): Result<String> {
        return suspendCancellableCoroutine { continuation ->
            integrityManager.requestIntegrityToken(
                IntegrityTokenRequest.builder()
                    .setCloudProjectNumber(BuildConfig.GOOGLE_PROJECT_ID)
                    .setNonce(nonce)
                    .build()
            )
                .addOnSuccessListener {
                    continuation.resume(Result.success(it.token())) {}
                }
                .addOnFailureListener { exception ->
                    continuation.resume(Result.failure(exception)) {}
                }
        }
    }
}
