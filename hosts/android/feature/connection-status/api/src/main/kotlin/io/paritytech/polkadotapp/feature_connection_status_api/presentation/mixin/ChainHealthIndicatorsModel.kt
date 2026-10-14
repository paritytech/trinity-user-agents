package io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin

import androidx.compose.runtime.Immutable
import kotlinx.collections.immutable.ImmutableList
import kotlin.math.roundToLong
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

enum class IndicatorRow {
    People,
    AssetHub,
    Bulletin,
    StatementStore,
}

/** The share of its expected blocks a connected chain produced, and the block interval that share implies. */
data class ChainLiveness(
    val share: Float,
    val blockInterval: Duration,
)

/**
 * What one row's indicator draws, in the priority the health rules give them: the connection first, then
 * block production. [Healthy] is a connected chain producing at least [HEALTHY_SHARE] of its expected blocks.
 */
sealed interface ChainHealthIndicator {
    /** [liveness] is null while the chain is connected but not yet measured. */
    data class Healthy(val liveness: ChainLiveness?) : ChainHealthIndicator

    /** Connected and short of blocks: the ring fills to the share, always above zero and below [HEALTHY_SHARE]. */
    data class Production(val liveness: ChainLiveness) : ChainHealthIndicator {
        val band: ProductionBand
            get() = ProductionBand.of(liveness.share)
    }

    /** Connected and produced nothing across the window. */
    data object Outage : ChainHealthIndicator

    data object Connecting : ChainHealthIndicator

    /** No node responding, or a connected node that no longer answers. */
    data object Disconnected : ChainHealthIndicator

    /** The device has no network. Drawn as [Disconnected] but named for the cause, not the chain. */
    data object Offline : ChainHealthIndicator

    companion object {
        /** Five sixths of the expected blocks; strictly below it the arc shows. */
        const val HEALTHY_SHARE = 5f / 6f

        /** The indicator for a connected chain from its production [share], null while unmeasured. */
        fun of(share: Float?, expectedBlockTime: Duration): ChainHealthIndicator = when {
            share == null -> Healthy(liveness = null)
            share <= 0f -> Outage
            else -> {
                val interval = (expectedBlockTime.inWholeMilliseconds / share).roundToLong().milliseconds
                val liveness = ChainLiveness(share = share, blockInterval = interval)
                if (share >= HEALTHY_SHARE) Healthy(liveness) else Production(liveness)
            }
        }
    }
}

/** Colour of a production arc. Each band starts at its [floor] share and runs up to the next band's. */
enum class ProductionBand(val floor: Float) {
    Error(0f),
    Warning(0.25f),
    Plain(0.5f),
    ;

    companion object {
        fun of(share: Float): ProductionBand = entries.last { share >= it.floor }
    }
}

@Immutable
data class ChainHealthIndicatorsModel(
    val rows: ImmutableList<ChainHealthItemModel>,
)

@Immutable
data class ChainHealthItemModel(
    val row: IndicatorRow,
    val indicator: ChainHealthIndicator,
)
