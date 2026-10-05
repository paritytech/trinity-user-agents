package io.paritytech.polkadotapp.feature_wallet_impl.data.config

import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

interface AppSharingConfigRepository {
    suspend fun getAppSharingUrl(): Result<String>
}

internal class RealAppSharingConfigRepository @Inject constructor(
    private val remoteConfigService: RemoteConfigService
) : AppSharingConfigRepository {
    override suspend fun getAppSharingUrl(): Result<String> {
        val synced = withTimeoutOrNull(APP_SHARING_URL_SYNC_WAIT) {
            remoteConfigService.getSyncedString(APP_SHARING_URL_KEY)
        } ?: Result.failure(IllegalStateException("Remote Config did not sync within $APP_SHARING_URL_SYNC_WAIT"))

        return synced.mapCatching { url ->
            require(url.isNotBlank()) { "Remote Config $APP_SHARING_URL_KEY is empty" }
            url
        }
    }

    private companion object {
        const val APP_SHARING_URL_KEY = "app_sharing_url"
        val APP_SHARING_URL_SYNC_WAIT = 3.seconds
    }
}
