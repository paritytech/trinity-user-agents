package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.flow.transformLatest
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime

/** Boundary state the smoother works on, decoupled from the substrate-sdk socket state. */
enum class RawConnectivity {
    Connected,
    Pending,
    Settled,
    Offline,
}

data class ConnectionSmoothingConfig(
    val lossHold: Duration,
    val flapWindow: Duration,
    val flapDropThreshold: Int,
    val flapHold: Duration,
    val sustainedDisconnect: Duration,
) {
    companion object {
        val Default = ConnectionSmoothingConfig(
            lossHold = 3.seconds,
            flapWindow = 30.seconds,
            flapDropThreshold = 2,
            flapHold = 5.seconds,
            sustainedDisconnect = 5.seconds,
        )
    }
}

/**
 * Hysteresis over the raw connectivity, biased towards the good news. A connect reports
 * [ChainConnectionPresentation.Connected] at once; a drop after it is held back for a loss hold,
 * extended while flapping, so a reconnect that lands inside the hold never shows. A settled disconnect
 * waits out a cooldown before reporting [ChainConnectionPresentation.Disconnected].
 */
@OptIn(ExperimentalCoroutinesApi::class, ExperimentalTime::class)
class ConnectionSmoother internal constructor(
    private val config: ConnectionSmoothingConfig,
    private val timeProvider: TimeProvider,
) {
    @Inject
    constructor(timeProvider: TimeProvider) : this(ConnectionSmoothingConfig.Default, timeProvider)

    fun smooth(source: Flow<RawConnectivity>): Flow<ChainConnectionPresentation> = flow {
        val drops = ArrayDeque<Long>()

        val transitions = source
            .runningFold(Transition(null, null)) { acc, state -> Transition(acc.current, state) }
            .drop(1)

        emitAll(
            transitions.transformLatest { (previous, current) ->
                val wasConnected = previous == RawConnectivity.Connected
                if (wasConnected && current != RawConnectivity.Connected) recordDrop(drops)

                when {
                    current == RawConnectivity.Connected -> emit(ChainConnectionPresentation.Connected)

                    // No cooldown: a device with no network has a known cause, unlike a socket that merely went quiet.
                    current == RawConnectivity.Offline -> emit(ChainConnectionPresentation.Offline)

                    current == RawConnectivity.Settled -> {
                        emit(ChainConnectionPresentation.Connecting)
                        delay(config.sustainedDisconnect)
                        emit(ChainConnectionPresentation.Disconnected)
                    }

                    else -> {
                        if (wasConnected) delay(lossHold(drops))
                        emit(ChainConnectionPresentation.Connecting)
                    }
                }
            }.distinctUntilChanged(),
        )
    }

    private fun lossHold(drops: ArrayDeque<Long>): Duration {
        purgeOld(drops, now())

        return if (drops.size >= config.flapDropThreshold) config.flapHold else config.lossHold
    }

    private fun now(): Long = timeProvider.now().toEpochMilliseconds()

    private fun recordDrop(drops: ArrayDeque<Long>) {
        val now = now()
        purgeOld(drops, now)
        drops.addLast(now)
    }

    private fun purgeOld(drops: ArrayDeque<Long>, now: Long) {
        val cutoff = now - config.flapWindow.inWholeMilliseconds
        while (drops.isNotEmpty() && drops.first() < cutoff) drops.removeFirst()
    }

    private data class Transition(val previous: RawConnectivity?, val current: RawConnectivity?)
}
