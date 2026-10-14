@file:OptIn(kotlin.time.ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_members_api.domain

import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollection
import io.paritytech.polkadotapp.feature_members_api.data.model.RingMembersState
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

/**
 * Result of evaluating whether the current member can submit `Members.self_include` to bypass
 * the onboarding queue.
 */
sealed interface SelfIncludeEligibility {
    /** Cannot submit `self_include` and never will from this state (feature off, not onboarding, chain busy). */
    object NotEligible : SelfIncludeEligibility

    /**
     * `self_include` is unnecessary: the collection's `OnboardingSize` is 1, so the offchain worker
     * includes the member without the self-inclusion delay. Proceed straight to ring inclusion.
     */
    object SelfIncludeNotNeeded : SelfIncludeEligibility

    /** Will be able to submit at [readyAt]; caller may schedule a precise wake-up there. */
    data class Waiting(val readyAt: Instant) : SelfIncludeEligibility

    /** May submit `self_include` now with [callValidAt] as the chain timestamp argument. */
    data class Eligible(val callValidAt: Instant) : SelfIncludeEligibility

    companion object {
        /**
         * Buffer added on top of chain-derived `readyAt` before considering the call valid.
         * Guards against `SelfInclusionTooEarly` if the best-block timestamp the app reads
         * lags slightly behind the timestamp the chain sees when validating the extrinsic.
         * Mirrors the iOS implementation's `bufferSeconds = 60`.
         */
        private val SELF_INCLUDE_BUFFER = 60.seconds

        fun evaluate(
            position: RingPosition?,
            collection: RingCollection?,
            ringsState: RingMembersState?,
            bestBlockTime: Instant?,
        ): SelfIncludeEligibility {
            val delay = collection?.selfInclusionDelay ?: return NotEligible
            if (position !is RingPosition.Onboarding) return NotEligible
            val now = bestBlockTime ?: return NotEligible
            if (ringsState?.isAppendOnly != true) return NotEligible

            val queuedAt = Instant.fromEpochSeconds(position.queuedAt)
            val readyAt = queuedAt + delay.toLong().seconds + SELF_INCLUDE_BUFFER
            return if (now >= readyAt) Eligible(callValidAt = now) else Waiting(readyAt)
        }
    }
}
