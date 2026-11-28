package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingDetail
import uniffi.truapi.FundingFact
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

class FundingLiveLineTest {
    private val now = Instant.fromEpochMilliseconds(1_800_000_000_000L)

    private fun parts(fact: FundingFact, offsetMs: Long?) =
        fundingLiveParts(FundingDetail(fact, offsetMs?.let { (now.toEpochMilliseconds() + it).toULong() }), lastHeardAt = now - 30.seconds, now = now)

    private fun res(id: Int, vararg args: Any) = FundingLivePart.Res(id, args.toList())

    private fun seconds(n: Int) = res(RCommon.string.funding_duration_seconds, n)

    @Test
    fun `confirmations count with the time of the last block`() {
        assertEquals(
            listOf(res(RCommon.string.funding_live_confirmation, 2, 3), res(RCommon.string.funding_live_last_block, seconds(4))),
            parts(FundingFact.Confirmations(2u, 3u), -4_000),
        )
    }

    @Test
    fun `a scheduled step counts down, then says any moment`() {
        assertEquals(listOf(res(RCommon.string.funding_live_time_left, seconds(12))), parts(FundingFact.Scheduled, 12_000))
        assertEquals(listOf(res(RCommon.string.funding_live_any_moment)), parts(FundingFact.Scheduled, -1_000))
    }

    @Test
    fun `an attempt names the next try`() {
        assertEquals(
            listOf(res(RCommon.string.funding_live_attempt_of, 2, 3), res(RCommon.string.funding_live_next_try, seconds(20))),
            parts(FundingFact.Attempt(2u, 3u), 20_000),
        )
    }

    @Test
    fun `the provider's own text is shown as given`() {
        assertEquals(
            listOf(FundingLivePart.Verbatim("Swapping PAS for CASH"), res(RCommon.string.funding_live_ago, seconds(3))),
            parts(FundingFact.Other("Swapping PAS for CASH"), -3_000),
        )
    }

    @Test
    fun `without a fact it says when the provider was last heard`() {
        assertEquals(listOf(res(RCommon.string.funding_last_heard, seconds(30))), fundingLiveParts(null, now - 30.seconds, now))
        assertEquals(listOf(res(RCommon.string.funding_last_heard, seconds(30))), parts(FundingFact.ExpectedBy, null))
        assertEquals(listOf(res(RCommon.string.funding_last_heard_now)), fundingLiveParts(null, now - 2.seconds, now))
        assertEquals(emptyList<FundingLivePart>(), fundingLiveParts(null, null, now))
    }
}
