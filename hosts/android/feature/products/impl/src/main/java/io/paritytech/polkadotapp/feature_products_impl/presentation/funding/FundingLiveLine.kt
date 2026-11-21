package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.annotation.StringRes
import uniffi.truapi.FundingDetail
import uniffi.truapi.FundingFact
import kotlin.time.Duration
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

private val JUST_NOW = 5.seconds
private val ROUND_UP = 999.milliseconds

/** One piece of the line under the band, joined with " · ". */
sealed interface FundingLivePart {
    /** A string resource; an argument may itself be a [FundingLivePart]. */
    data class Res(@param:StringRes val id: Int, val args: List<Any>) : FundingLivePart

    /** The provider's own text, shown as given. */
    data class Verbatim(val text: String) : FundingLivePart

    /** A time, as "14:02" today or "Tue 14:02" otherwise, or as a day when [dayOnly]. */
    data class Moment(val at: Instant, val dayOnly: Boolean) : FundingLivePart
}

/**
 * The line under the band at [now]: the provider's latest [detail] for the current step, worded here,
 * or "Checked with the provider 12s ago" from [lastHeardAt]. Empty when there is neither.
 */
fun fundingLiveParts(detail: FundingDetail?, lastHeardAt: Instant?, now: Instant): List<FundingLivePart> {
    val at = detail?.atMs?.let { Instant.fromEpochMilliseconds(it.toLong()) }
    val ago = at?.takeIf { it <= now }?.let { duration(now - it) }
    // Rounded up to the second, so "1s left" shows until the moment comes.
    val left = at?.takeIf { it > now }?.let { duration(it - now + ROUND_UP) }
    fun res(@StringRes id: Int, vararg args: Any) = FundingLivePart.Res(id, args.toList())
    val parts: List<FundingLivePart?> = when (val fact = detail?.fact) {
        null, FundingFact.Checked -> listOf((at ?: lastHeardAt)?.let { lastHeard(it, now) })
        is FundingFact.Confirmations -> listOf(
            res(RCommon.string.funding_live_confirmation, fact.done.toInt(), fact.required.toInt()),
            ago?.let { res(RCommon.string.funding_live_last_block, it) },
        )
        FundingFact.PriceChecked -> listOf(res(RCommon.string.funding_live_price_checked), ago?.let { res(RCommon.string.funding_live_ago, it) })
        is FundingFact.Parts -> listOf(
            res(RCommon.string.funding_live_part, fact.done.toInt(), fact.total.toInt()),
            ago?.let { res(RCommon.string.funding_live_last_part, it) },
        )
        FundingFact.Scheduled -> listOf(left?.let { res(RCommon.string.funding_live_time_left, it) } ?: res(RCommon.string.funding_live_any_moment))
        FundingFact.SentByBank -> listOf(res(RCommon.string.funding_live_sent_by_bank), at?.let { FundingLivePart.Moment(it, dayOnly = false) })
        FundingFact.ExpectedBy -> listOf(at?.let { res(RCommon.string.funding_live_expected_by, FundingLivePart.Moment(it, dayOnly = true)) })
        FundingFact.InReview -> listOf(res(RCommon.string.funding_live_in_review), ago?.let { res(RCommon.string.funding_live_ago, it) })
        is FundingFact.Attempt -> listOf(
            fact.of?.let { res(RCommon.string.funding_live_attempt_of, fact.n.toInt(), it.toInt()) }
                ?: res(RCommon.string.funding_live_attempt, fact.n.toInt()),
            left?.let { res(RCommon.string.funding_live_next_try, it) },
        )
        is FundingFact.Other -> listOf(FundingLivePart.Verbatim(fact.text), ago?.let { res(RCommon.string.funding_live_ago, it) })
    }
    return parts.filterNotNull().ifEmpty { listOfNotNull(lastHeardAt?.let { lastHeard(it, now) }) }
}

private fun lastHeard(at: Instant, now: Instant): FundingLivePart.Res {
    val since = (now - at).coerceAtLeast(Duration.ZERO)
    return when {
        since < JUST_NOW -> FundingLivePart.Res(RCommon.string.funding_last_heard_now, emptyList())
        else -> FundingLivePart.Res(RCommon.string.funding_last_heard, listOf(duration(since)))
    }
}

/** "12s", "3 min" or "2 h". */
private fun duration(length: Duration): FundingLivePart.Res = when {
    length < 1.minutes -> FundingLivePart.Res(RCommon.string.funding_duration_seconds, listOf(length.inWholeSeconds.toInt()))
    length < 1.hours -> FundingLivePart.Res(RCommon.string.funding_duration_minutes, listOf(length.inWholeMinutes.toInt()))
    else -> FundingLivePart.Res(RCommon.string.funding_duration_hours, listOf(length.inWholeHours.toInt()))
}
