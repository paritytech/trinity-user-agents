package io.paritytech.polkadotapp.tools_media_connection_impl.turn

import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.tools_media_connection_impl.models.ExternalRtcConfig
import io.paritytech.polkadotapp.tools_media_connection_impl.models.TurnCredentials
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

@Singleton
internal class ExternalRtcConfigProvider @Inject constructor(
    private val turnApi: TurnApi,
    private val currentTimeContext: CurrentTimeContext
) {
    private val mutex = Mutex()
    private var cached: CachedConfig? = null
    private var inflight: Fetch? = null

    suspend fun getConfig(waitTimeout: Duration = Duration.INFINITE): ExternalRtcConfig {
        currentCoroutineContext().ensureActive()
        val request = mutex.withLock {
            val fresh = cached?.takeIf { currentTimeContext.currentTime() < it.expiry }
            if (fresh != null) return fresh.config

            val pending = inflight ?: startFetch()
            pending.waiters++
            pending
        }
        var keepForCache = false
        try {
            request.task.start()
            val config = withTimeoutOrNull(waitTimeout) { request.task.await() }
            currentCoroutineContext().ensureActive()
            keepForCache = config == null
            return config ?: ExternalRtcConfig(turnCredentials = emptyList())
        } finally {
            withContext(NonCancellable) {
                mutex.withLock {
                    request.waiters--
                    request.keepForCache = request.keepForCache || keepForCache
                    if (request.waiters == 0 && !request.keepForCache && inflight === request) {
                        inflight = null
                        request.task.cancel()
                    }
                }
            }
        }
    }

    private suspend fun startFetch(): Fetch {
        val task = CoroutineScope(currentCoroutineContext().minusKey(Job)).async(start = CoroutineStart.LAZY) {
            try {
                fetchConfig(currentTimeContext.currentTime())
            } finally {
                val job = currentCoroutineContext()[Job]
                withContext(NonCancellable) {
                    mutex.withLock {
                        if (inflight?.task === job) inflight = null
                    }
                }
            }
        }
        return Fetch(task).also { inflight = it }
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
                mutex.withLock {
                    currentCoroutineContext().ensureActive()
                    cached = CachedConfig(
                        config = config,
                        expiry = now + (response.ttl - EXPIRY_BUFFER_SECONDS).seconds
                    )
                }
                Timber.i("TURN: fetched ${config.turnCredentials.size} ICE server(s)")
                config
            },
            onFailure = { error ->
                Timber.w(error, "TURN: failed to fetch credentials, using empty list")
                ExternalRtcConfig(turnCredentials = emptyList())
            }
        )

    private data class CachedConfig(val config: ExternalRtcConfig, val expiry: Instant)

    private class Fetch(val task: Deferred<ExternalRtcConfig>) {
        var waiters = 0
        var keepForCache = false
    }

    companion object {
        private const val EXPIRY_BUFFER_SECONDS = 5
        private val FETCH_TIMEOUT = 15.seconds
    }

    private fun TurnCredentialsResponse.toExternalRtcConfig(): ExternalRtcConfig =
        ExternalRtcConfig(
            turnCredentials = servers.map { TurnCredentials(url = it, username = username, password = password) }
        )
}
