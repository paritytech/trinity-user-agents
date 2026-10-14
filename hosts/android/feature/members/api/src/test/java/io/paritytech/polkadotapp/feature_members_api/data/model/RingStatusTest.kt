package io.paritytech.polkadotapp.feature_members_api.data.model

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * A ring's keys are stored in pages of [KEYS_PER_PAGE]; a member's position is counted within its page, while
 * [RingStatus.included] counts across the whole ring.
 */
class RingStatusTest {
    @Test
    fun `a key on the first page is included once the ring has baked past it`() {
        assertTrue(RingStatus(total = 300, included = 101).includesKey(keysPerPage = KEYS_PER_PAGE, position = positionOf(page = 0, position = 100)))
    }

    @Test
    fun `a key on the first page is not included before the ring has baked it`() {
        assertFalse(RingStatus(total = 300, included = 100).includesKey(keysPerPage = KEYS_PER_PAGE, position = positionOf(page = 0, position = 100)))
    }

    /** Page 1, position 153 is the ring's 409th key, so 408 baked keys stop just short of it. */
    @Test
    fun `a key on a later page counts the pages before it`() {
        assertFalse(RingStatus(total = 409, included = 408).includesKey(keysPerPage = KEYS_PER_PAGE, position = positionOf(page = 1, position = 153)))
        assertTrue(RingStatus(total = 409, included = 409).includesKey(keysPerPage = KEYS_PER_PAGE, position = positionOf(page = 1, position = 153)))
    }

    @Test
    fun `a key that is still onboarding is not included`() {
        assertFalse(RingStatus(total = 409, included = 409).includesKey(keysPerPage = KEYS_PER_PAGE, position = RingPosition.Onboarding(queuePage = 0, queuedAt = 0)))
    }

    private fun positionOf(page: Int, position: Int) =
        RingPosition.Included(ringIndex = RingIndex(0.toBigInteger()), ringPage = page, ringPosition = position)

    private companion object {
        const val KEYS_PER_PAGE = 255
    }
}
