package io.paritytech.polkadotapp.chains.multiNetwork.connection

import io.novasama.substrate_sdk_android.wsrpc.state.SocketStateMachine.State
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

data class ShortLivedConnectionConfig(
    val maxShortLife: Duration,
    val dropsBeforeSwitch: Int,
) {
    companion object {
        val Default = ShortLivedConnectionConfig(
            maxShortLife = 5.seconds,
            dropsBeforeSwitch = 2,
        )
    }
}

/**
 * Asks for a node switch when the current node keeps accepting the socket and dropping it right away.
 *
 * The SDK schedules the reconnect after a drop from [State.Connected] with the attempt counter reset
 * to 0, so such a node never reaches the attempt threshold that [ChainConnection] rotates on, and the
 * socket loops on it until the app restarts.
 */
@OptIn(ExperimentalTime::class)
class ShortLivedConnectionDetector internal constructor(
    private val config: ShortLivedConnectionConfig,
    private val timeProvider: TimeProvider,
) {
    @Inject
    constructor(timeProvider: TimeProvider) : this(ShortLivedConnectionConfig.Default, timeProvider)

    fun nodeChangeEvents(states: Flow<State>): Flow<Unit> = flow {
        var trackedUrl: String? = null
        var connectedAt: Instant? = null
        var shortDrops = 0

        states.collect { state ->
            when (state) {
                is State.Connected -> {
                    if (state.url != trackedUrl) {
                        trackedUrl = state.url
                        shortDrops = 0
                    }
                    connectedAt = timeProvider.now()
                }

                is State.WaitingForReconnect -> {
                    val droppedAfter = connectedAt?.let { timeProvider.now() - it }
                    connectedAt = null

                    if (droppedAfter != null) {
                        shortDrops = if (droppedAfter < config.maxShortLife) shortDrops + 1 else 0

                        if (shortDrops >= config.dropsBeforeSwitch) {
                            Timber.d("$trackedUrl dropped the socket right after accepting it $shortDrops times in a row, switching node")
                            shortDrops = 0
                            emit(Unit)
                        }
                    }
                }

                else -> connectedAt = null
            }
        }
    }
}
