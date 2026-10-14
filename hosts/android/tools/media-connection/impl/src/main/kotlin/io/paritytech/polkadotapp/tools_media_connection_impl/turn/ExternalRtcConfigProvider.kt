package io.paritytech.polkadotapp.tools_media_connection_impl.turn

import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.tools_media_connection_impl.models.ExternalRtcConfig
import io.paritytech.polkadotapp.tools_media_connection_impl.models.TurnCredentials
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

@Singleton
internal class ExternalRtcConfigProvider @Inject constructor(
    private val turnApi: TurnApi,
    private val currentTimeContext: CurrentTimeContext
) {
    private val mutex = Mutex()
    private var cached: CachedConfig? = null

    suspend fun getConfig(): ExternalRtcConfig = mutex.withLock {
        val now = currentTimeContext.currentTime()

        val fresh = cached?.takeIf { now < it.expiry }
        if (fresh != null) return@withLock fresh.config

        fetchConfig(now)
    }

    private suspend fun fetchConfig(now: Instant): ExternalRtcConfig =
        runCancellableCatching {
            withTimeoutOrNull(FETCH_TIMEOUT) {
                turnApi.issueTurnCredentials(TurnIssueRequestBody(regionHint = null))
            }
        }.fold(
            onSuccess = { response ->
                if (response == null) {
                    Timber.w("TURN: fetch timed out after $FETCH_TIMEOUT, using empty list")
                    return@fold ExternalRtcConfig(turnCredentials = emptyList())
                }

                val config = response.toExternalRtcConfig()
                cached = CachedConfig(
                    config = config,
                    expiry = now + (response.ttl - EXPIRY_BUFFER_SECONDS).seconds
                )
                Timber.i("TURN: fetched ${config.turnCredentials.size} ICE server(s)")
                config
            },
            onFailure = { error ->
                Timber.w(error, "TURN: failed to fetch credentials, using empty list")
                ExternalRtcConfig(turnCredentials = emptyList())
            }
        )

    private data class CachedConfig(val config: ExternalRtcConfig, val expiry: Instant)

    companion object {
        private const val EXPIRY_BUFFER_SECONDS = 5
        private val FETCH_TIMEOUT = 5.seconds
    }

    private fun TurnCredentialsResponse.toExternalRtcConfig(): ExternalRtcConfig =
        ExternalRtcConfig(
            turnCredentials = servers.map { TurnCredentials(url = it, username = username, password = password) }
        )
}
