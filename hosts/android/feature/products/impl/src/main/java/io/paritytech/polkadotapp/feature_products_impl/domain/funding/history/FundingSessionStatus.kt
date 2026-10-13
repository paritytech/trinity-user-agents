package io.paritytech.polkadotapp.feature_products_impl.domain.funding.history

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isOpen
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingStep
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

/**
 * The one line a session in flight shows, read from the provider's updates and the core's progress.
 *
 * This is the only place the host derives it. It stands in for the typed status the core is to give every
 * session, and goes once that lands.
 */
internal object FundingSessionStatus {
    /** Still in flight: open, or outbound value released whose payout the provider has not reported yet. */
    fun isInFlight(session: FundingSession, progress: FundingProgress?): Boolean =
        session.stage.isOpen || awaitsPayout(session, progress)

    fun of(session: FundingSession, progress: FundingProgress?, now: Instant): FundingActivityStatus.InProgress {
        val step = step(session, progress)
        val retrying = step == FundingActivityStep.Retrying || step == FundingActivityStep.RetryingPayout
        return FundingActivityStatus.InProgress(step = step, delayed = !retrying && isDelayed(session, now))
    }

    private fun awaitsPayout(session: FundingSession, progress: FundingProgress?): Boolean =
        session.direction == FundingDirection.OUT && session.stage is FundingStage.Released && progress?.payout == null

    private fun step(session: FundingSession, progress: FundingProgress?): FundingActivityStep {
        val rail = session.choice?.rail

        return when (session.direction) {
            FundingDirection.IN -> {
                val reached = progress?.steps.orEmpty().filter { it.reachedAtMs != null }.map { it.step }.toSet()
                when {
                    progress?.retrying == true -> FundingActivityStep.Retrying
                    FundingStep.PAYMENT in reached || FundingStep.CONVERSION in reached -> FundingActivityStep.Converting
                    rail == FundingRail.BANK -> FundingActivityStep.WaitingForTransfer
                    rail == FundingRail.CRYPTO -> FundingActivityStep.WaitingForDeposit
                    else -> FundingActivityStep.WaitingForPayment
                }
            }

            FundingDirection.OUT -> when {
                progress?.retrying == true -> FundingActivityStep.RetryingPayout
                session.stage !is FundingStage.Released -> FundingActivityStep.TransactionInitiated
                rail == FundingRail.CRYPTO -> FundingActivityStep.ConvertingOut(session.choice?.asset.orEmpty())
                else -> FundingActivityStep.PreparingPayout
            }
        }
    }

    /** Slower than the provider said: nothing reported for longer than the time the chosen quote gave. */
    private fun isDelayed(session: FundingSession, now: Instant): Boolean {
        val eta = session.choice?.quote?.etaSecs ?: return false
        val latestMs = session.updates.maxOfOrNull { it.atMs } ?: session.openedAtMs
        return now - Instant.fromEpochMilliseconds(latestMs.toLong()) > eta.toLong().seconds
    }
}
