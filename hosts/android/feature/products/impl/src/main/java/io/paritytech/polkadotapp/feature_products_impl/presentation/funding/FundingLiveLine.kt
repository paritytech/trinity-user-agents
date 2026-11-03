package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.annotation.StringRes
import uniffi.truapi.FundingDetail
import uniffi.truapi.FundingFact
import io.paritytech.polkadotapp.common.R as RCommon

private const val SECONDS_PER_MINUTE = 60L
private const val SECONDS_PER_HOUR = 3_600L
private const val JUST_NOW_SECONDS = 5L

/** One piece of the line under the band, joined with " · ". */
sealed interface FundingLivePart {
    /** A string resource; an argument may itself be a [FundingLivePart]. */
    data class Res(@param:StringRes val id: Int, val args: List<Any> = emptyList()) : FundingLivePart

    /** The provider's own text, shown as given. */
    data class Verbatim(val text: String) : FundingLivePart

    /** A time, as "14:02" today or "Tue 14:02" otherwise, or as a day when [dayOnly]. */
    data class Moment(val atMs: Long, val dayOnly: Boolean) : FundingLivePart
}

/**
 * The line under the band at [nowMs]: the provider's latest [detail] for the current step, worded here,
 * or "Checked with the provider 12s ago" from [lastHeardAtMs]. Empty when there is neither.
 */
fun fundingLiveParts(detail: FundingDetail?, lastHeardAtMs: Long?, nowMs: Long): List<FundingLivePart> {
    val atMs = detail?.atMs?.toLong()
    val ago = atMs?.takeIf { it <= nowMs }?.let { duration((nowMs - it) / 1_000L) }
    val left = atMs?.takeIf { it > nowMs }?.let { duration((it - nowMs + 999L) / 1_000L) }
    fun res(@StringRes id: Int, vararg args: Any) = FundingLivePart.Res(id, args.toList())
    val parts: List<FundingLivePart?> = when (val fact = detail?.fact) {
        null, FundingFact.Checked -> listOf((atMs ?: lastHeardAtMs)?.let { lastHeard(it, nowMs) })
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
        FundingFact.SentByBank -> listOf(res(RCommon.string.funding_live_sent_by_bank), atMs?.let { FundingLivePart.Moment(it, dayOnly = false) })
        FundingFact.ExpectedBy -> listOf(atMs?.let { res(RCommon.string.funding_live_expected_by, FundingLivePart.Moment(it, dayOnly = true)) })
        FundingFact.InReview -> listOf(res(RCommon.string.funding_live_in_review), ago?.let { res(RCommon.string.funding_live_ago, it) })
        is FundingFact.Attempt -> listOf(
            fact.of?.let { res(RCommon.string.funding_live_attempt_of, fact.n.toInt(), it.toInt()) }
                ?: res(RCommon.string.funding_live_attempt, fact.n.toInt()),
            left?.let { res(RCommon.string.funding_live_next_try, it) },
        )
        is FundingFact.Other -> listOf(FundingLivePart.Verbatim(fact.text), ago?.let { res(RCommon.string.funding_live_ago, it) })
    }
    return parts.filterNotNull().ifEmpty { listOfNotNull(lastHeardAtMs?.let { lastHeard(it, nowMs) }) }
}

private fun lastHeard(atMs: Long, nowMs: Long): FundingLivePart.Res {
    val seconds = ((nowMs - atMs) / 1_000L).coerceAtLeast(0)
    return when {
        seconds < JUST_NOW_SECONDS -> FundingLivePart.Res(RCommon.string.funding_last_heard_now)
        else -> FundingLivePart.Res(RCommon.string.funding_last_heard, listOf(duration(seconds)))
    }
}

/** "12s", "3 min" or "2 h". */
private fun duration(seconds: Long): FundingLivePart.Res = when {
    seconds < SECONDS_PER_MINUTE -> FundingLivePart.Res(RCommon.string.funding_duration_seconds, listOf(seconds.toInt()))
    seconds < SECONDS_PER_HOUR -> FundingLivePart.Res(RCommon.string.funding_duration_minutes, listOf((seconds / SECONDS_PER_MINUTE).toInt()))
    else -> FundingLivePart.Res(RCommon.string.funding_duration_hours, listOf((seconds / SECONDS_PER_HOUR).toInt()))
}
