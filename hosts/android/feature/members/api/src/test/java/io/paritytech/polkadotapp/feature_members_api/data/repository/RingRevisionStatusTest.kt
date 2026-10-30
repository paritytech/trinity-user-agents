package io.paritytech.polkadotapp.feature_members_api.data.repository

import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import org.junit.Assert.assertEquals
import org.junit.Test

class RingRevisionStatusTest {
    private fun held(vararg revisions: Int) = revisions.map(::RingRevision)

    @Test
    fun `a held revision is imported`() {
        assertEquals(RingRevisionStatus.IMPORTED, ringRevisionStatus(held(105, 106, 108), RingRevision(106)))
    }

    @Test
    fun `a revision newer than the window is pending`() {
        assertEquals(RingRevisionStatus.PENDING, ringRevisionStatus(held(), RingRevision(106)))
        assertEquals(RingRevisionStatus.PENDING, ringRevisionStatus(held(103, 104, 105), RingRevision(106)))
    }

    @Test
    fun `a skipped revision is pruned rather than pending`() {
        assertEquals(RingRevisionStatus.PRUNED, ringRevisionStatus(held(108, 105, 106), RingRevision(107)))
    }

    @Test
    fun `a revision older than the window is pruned`() {
        assertEquals(RingRevisionStatus.PRUNED, ringRevisionStatus(held(105, 106, 108), RingRevision(42)))
    }
}
