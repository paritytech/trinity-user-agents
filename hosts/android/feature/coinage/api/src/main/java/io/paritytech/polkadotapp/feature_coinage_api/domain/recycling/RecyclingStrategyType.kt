package io.paritytech.polkadotapp.feature_coinage_api.domain.recycling

import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import kotlin.math.max
import kotlin.time.Duration.Companion.minutes

/** The privacy strategies offered to the user. */
enum class RecyclingStrategyType {
    MIN_PRIVACY,
    BALANCED,
    MAX_PRIVACY
}

/**
 * The presets express their recycling age against the age at which the chain stops accepting a coin, rather
 * than hardcoding ages of their own — see [MinRecyclingAge]. Reading that limit is the strategy's job, which
 * is what lets a preset be built without touching the chain.
 */
val RecyclingStrategyType.params: RecyclingParams
    get() = when (this) {
        RecyclingStrategyType.MIN_PRIVACY -> RecyclingParams(
            maxUnavailableBalance = Fraction.ZERO,
            minRecyclingAge = MinRecyclingAge.UseChainLimit(divisor = 1),
            voucherReadiness = VoucherReadiness(requiredRingFill = Fraction.ZERO, memberAndAgeRequirements = null),
            // Nothing is ever held back under this strategy, so the offer has nothing to apply to.
            allowsConfirmedSpend = true,
        )

        RecyclingStrategyType.BALANCED -> RecyclingParams(
            maxUnavailableBalance = BALANCED_UNAVAILABLE_BALANCE,
            minRecyclingAge = MinRecyclingAge.UseChainLimit(divisor = BALANCED_AGE_DIVISOR),
            voucherReadiness = VoucherReadiness(requiredRingFill = BALANCED_RING_FILL, memberAndAgeRequirements = MEMBER_AND_AGE_REQUIREMENTS),
            allowsConfirmedSpend = true,
        )

        RecyclingStrategyType.MAX_PRIVACY -> RecyclingParams(
            maxUnavailableBalance = Fraction.FULL,
            minRecyclingAge = MinRecyclingAge.Override(MIN_RECYCLABLE_AGE),
            voucherReadiness = VoucherReadiness(requiredRingFill = MAX_PRIVACY_RING_FILL, memberAndAgeRequirements = MEMBER_AND_AGE_REQUIREMENTS),
            allowsConfirmedSpend = false,
        )
    }

/** [forcedRecyclingAge] is the age at which the chain stops accepting a coin. */
fun MinRecyclingAge.resolveAgainst(forcedRecyclingAge: Int): Int = when (this) {
    is MinRecyclingAge.Override -> age
    is MinRecyclingAge.UseChainLimit -> max(MIN_RECYCLABLE_AGE, forcedRecyclingAge / divisor)
}

/**
 * An unload mints its successor at age 1, so no coin younger than this exists on chain to recycle. Maximum
 * privacy sitting here means a recycled coin's successor is eligible again immediately — that is the point.
 */
private const val MIN_RECYCLABLE_AGE = 1

private const val BALANCED_AGE_DIVISOR = 3

private val BALANCED_UNAVAILABLE_BALANCE = 20.percents

private val BALANCED_RING_FILL = 20.percents

private val MAX_PRIVACY_RING_FILL = 90.percents

private val MEMBER_AND_AGE_REQUIREMENTS = MemberAndAgeRequirements(minimumMembers = 32, delay = 10.minutes)
