package io.paritytech.polkadotapp.feature_videogame_impl.data.tracked

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.storage.source.query.intercept.StorageQueryRequest
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.ActiveTrackedExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.ExtrinsicTag
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.TrackedExtrinsicService
import io.paritytech.polkadotapp.feature_videogame_api.domain.state.model.GameIndex
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainGamePlayerCredibility
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainVideoGamePlayerInfo
import io.paritytech.polkadotapp.feature_videogame_impl.domain.tracked.VideoGameExtrinsicTags
import io.paritytech.polkadotapp.feature_videogame_impl.domain.tracked.VideoGamePlayerOverrideTarget
import io.paritytech.polkadotapp.feature_videogame_impl.domain.tracked.encodeToAdditional
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalTxOverrideInterceptorTest {
    private val ourKey = "0xOUR_PLAYER_KEY"
    private val otherKey = "0xOTHER_PLAYER_KEY"
    private val gameIndex = GameIndex(7)

    @Test
    fun `chain-true value stays true`() = runBlocking<Unit> {
        val chainValue = playerInfo(registered = true, sentReport = true)
        val interceptor = interceptor(vote = activeFor(ourKey), register = activeFor(ourKey))

        val result = interceptor.run(chainValue)!!

        assertTrue(result.registered)
        assertTrue(result.sentReport)
        assertEquals(chainValue.firstGame, result.firstGame)
    }

    @Test
    fun `chain-false plus active vote targeting this key sets sentReport`() = runBlocking<Unit> {
        val interceptor = interceptor(vote = activeFor(ourKey), register = null)

        val result = interceptor.run(playerInfo(registered = false, sentReport = false))!!

        assertTrue(result.sentReport)
        assertFalse(result.registered)
    }

    @Test
    fun `chain-null plus active register targeting this key synthesizes a registered row`() = runBlocking<Unit> {
        val interceptor = interceptor(vote = null, register = activeFor(ourKey))

        val result = interceptor.run(null)

        assertNotNull(result)
        assertTrue(result!!.registered)
        assertFalse(result.sentReport)
        assertEquals(gameIndex, result.firstGame)
    }

    @Test
    fun `active tx targeting a different key is ignored`() = runBlocking<Unit> {
        val chainValue = playerInfo(registered = false, sentReport = false)
        val interceptor = interceptor(vote = activeFor(otherKey), register = activeFor(otherKey))

        val result = interceptor.run(chainValue)

        assertEquals(chainValue, result)
    }

    @Test
    fun `no active tx leaves the value untouched`() = runBlocking<Unit> {
        assertNull(interceptor(vote = null, register = null).run(null))

        val chainValue = playerInfo(registered = false, sentReport = false)
        assertEquals(chainValue, interceptor(vote = null, register = null).run(chainValue))
    }

    @Test
    fun `vote and register overrides are independent`() = runBlocking<Unit> {
        val voteOnly = interceptor(vote = activeFor(ourKey), register = null)
            .run(playerInfo(registered = false, sentReport = false))!!
        assertTrue(voteOnly.sentReport)
        assertFalse(voteOnly.registered)

        val registerOnly = interceptor(vote = null, register = activeFor(ourKey))
            .run(playerInfo(registered = false, sentReport = false))!!
        assertTrue(registerOnly.registered)
        assertFalse(registerOnly.sentReport)
    }

    private suspend fun LocalTxOverrideInterceptor.run(chainValue: OnChainVideoGamePlayerInfo?): OnChainVideoGamePlayerInfo? {
        return interceptQuery(
            StorageQueryRequest(
                module = "Game",
                storage = "Players",
                storageKey = ourKey,
                keyArguments = emptyList(),
                value = chainValue,
            )
        )
    }

    private suspend fun interceptor(vote: ActiveTrackedExtrinsic?, register: ActiveTrackedExtrinsic?): LocalTxOverrideInterceptor {
        val service = mockk<TrackedExtrinsicService>()
        coEvery { service.getLatestActive(VideoGameExtrinsicTags.VOTE_PREFIX.value) } returns vote
        coEvery { service.getLatestActive(VideoGameExtrinsicTags.REGISTER_PREFIX.value) } returns register

        return LocalTxOverrideInterceptor(service)
    }

    private fun playerInfo(registered: Boolean, sentReport: Boolean) =
        OnChainVideoGamePlayerInfo(
            firstGame = GameIndex(1),
            registered = registered,
            sentReport = sentReport,
            earlyAttendanceEnactment = null,
            yesPerson = 0,
            noNotPerson = 0,
            expectedMaxVoteWeight = 0,
            voteWeight = 0,
            credibility = OnChainGamePlayerCredibility.Invited,
        )

    private fun activeFor(storageKey: String): ActiveTrackedExtrinsic {
        val additional: DataByteArray = VideoGamePlayerOverrideTarget(storageKey, gameIndex).encodeToAdditional()
        return ActiveTrackedExtrinsic(ExtrinsicTag("tag-$storageKey"), additional)
    }
}
