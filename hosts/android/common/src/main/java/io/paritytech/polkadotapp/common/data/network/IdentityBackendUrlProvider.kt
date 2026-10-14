package io.paritytech.polkadotapp.common.data.network

/**
 * Placeholder base URL handed to Retrofit for identity-backend services. The real host is resolved
 * per-request from remote config by [IdentityBackendUrlInterceptor]; requests are only rewritten
 * when their host matches [IDENTITY_BACKEND_SENTINEL_HOST], so other base URLs pass through untouched.
 */
const val IDENTITY_BACKEND_SENTINEL_HOST = "identity-backend.sentinel"
const val IDENTITY_BACKEND_SENTINEL_URL = "https://$IDENTITY_BACKEND_SENTINEL_HOST/"

/**
 * Resolves the identity-backend base URL. The implementation reads it from remote config and lives
 * outside `common` (bound via Hilt), so `common` stays free of a remote-config dependency.
 */
interface IdentityBackendUrlProvider {
    suspend fun getBaseUrl(): Result<String>
}
