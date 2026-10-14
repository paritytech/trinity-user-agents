package io.paritytech.polkadotapp.feature_coinage_api.domain.recycling

import io.paritytech.polkadotapp.common.utils.Fraction
import kotlin.time.Duration

/**
 * How aggressively coins are moved through the recycler.
 *
 * The named strategies are points in this space rather than separate policies, so a later release can open
 * up the space itself — intermediate positions, then both axes independently — without the gating logic
 * changing.
 */
data class RecyclingParams(
    /**
     * Share of the balance that may be unavailable at once while recycling.
     *
     * A ceiling, not a target: it exists so more than one coin can be in flight. Coins the chain's age limit
     * forces ignore it, which is why the minimum-privacy strategy can set it to zero and still recycle.
     */
    val maxUnavailableBalance: Fraction,
    /** Coins below this age are not considered for recycling at all. */
    val minRecyclingAge: MinRecyclingAge,
    /** Readiness rules for vouchers already included in a recycler ring. */
    val voucherReadiness: VoucherReadiness,
    /**
     * Whether balance held back for privacy may still be spent, once the user has confirmed they accept the
     * loss. False for the strategy whose whole point is that it will not.
     */
    val allowsConfirmedSpend: Boolean,
)

/**
 * The age a preset starts recycling at, stated so that building the params needs no chain read.
 *
 * A preset that tracks the chain's own limit cannot name an age up front — the runtime may move it — so it
 * describes the threshold here and the strategy resolves it when it is asked for a verdict.
 */
sealed interface MinRecyclingAge {
    /**
     * The age the chain stops accepting a coin at, divided by [divisor].
     *
     * A divisor of one is the limit itself. Anything softer is a share of it, so the preset stays correct
     * relative to the chain rather than drifting when the runtime changes the limit.
     */
    data class UseChainLimit(val divisor: Int) : MinRecyclingAge

    /** A fixed age, for a preset whose threshold is deliberately unrelated to the chain's limit. */
    data class Override(val age: Int) : MinRecyclingAge
}

/** A voucher is ready when either requirement is satisfied. */
data class VoucherReadiness(
    /** Required ring fill for immediate readiness. */
    val requiredRingFill: Fraction,
    /** Alternative to the fill threshold; null means readiness depends on ring fill alone. */
    val memberAndAgeRequirements: MemberAndAgeRequirements?,
)

/**
 * Allows readiness below the ring-fill threshold once both membership and waiting requirements are met.
 */
data class MemberAndAgeRequirements(
    /** Included members required for the timed route, regardless of how long the voucher has waited. */
    val minimumMembers: Int,
    /** Time since first confirmed inclusion; waiting to reach [minimumMembers] counts toward this delay. */
    val delay: Duration,
)
