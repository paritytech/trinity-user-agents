package io.paritytech.polkadotapp.database.dao

import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.model.RecyclerVoucherLocal
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * What one UPDATE has to guarantee about a voucher's ring: when its timer starts, and when its fungibility
 * may be left alone.
 *
 * The frozen maximum records the anonymity a ring offered when the voucher arrived; it is what makes a later,
 * higher current fungibility legible as the overloaded state the UI absorbs rather than as corruption. If it
 * could move, that reading would be gone. And a null parameter has to leave a stored fungibility standing,
 * because the value needs two chain reads that can fail independently of the location written alongside it.
 */
@RunWith(AndroidJUnit4::class)
class RecyclerVoucherDaoTest {

    private val publicKey = byteArrayOf(0x09)

    private lateinit var database: AppDatabase

    @Before
    fun setUp() {
        database = Room.inMemoryDatabaseBuilder(
            InstrumentationRegistry.getInstrumentation().targetContext,
            AppDatabase::class.java
        ).build()
    }

    @After
    fun tearDown() = database.close()

    @Test
    fun repeatedObservationsKeepTheFirstConfirmedInclusionTime() = runBlocking {
        givenVoucher()

        update(enteredAt = 1_000L, recyclerMembers = 31)
        update(enteredAt = 601_000L, recyclerMembers = 32)

        assertEquals(1_000L, storedVoucher().enteredAt)
    }

    @Test
    fun movingToAnotherRingStartsANewTimer() = runBlocking {
        givenVoucher(ring = RING_INDEX, members = 32, enteredAt = 1_000L)

        update(recyclerIndex = RING_INDEX + 1, enteredAt = 601_000L, recyclerMembers = 40)

        val voucher = storedVoucher()
        assertEquals(RING_INDEX + 1, voucher.locationRecyclerIndex)
        assertEquals(601_000L, voucher.enteredAt)
    }

    @Test
    fun restoreRestartsTheTimerAtTheNextConfirmedObservation() = runBlocking {
        givenVoucher(ring = RING_INDEX, members = 32, enteredAt = 1_000L)
        // What a restore leaves behind: the ring is known again, the time in it is not.
        dao().insertAll(listOf(voucherRow(ring = RING_INDEX, members = 0, enteredAt = null)))
        assertNull(storedVoucher().enteredAt)

        update(enteredAt = 61_000L, recyclerMembers = 32)
        update(enteredAt = 121_000L, recyclerMembers = 33)

        assertEquals(61_000L, storedVoucher().enteredAt)
    }

    @Test
    fun theFirstUpdateFreezesTheMaximum() = runBlocking {
        givenVoucher()

        update(recyclerFungibility = 90, maxRecyclerFungibility = 90)

        assertEquals(90, storedVoucher().maxRecyclerFungibility)
    }

    @Test
    fun alaterUpdateLeavesTheFrozenMaximumAlone() = runBlocking {
        givenVoucher()

        update(recyclerFungibility = 90, maxRecyclerFungibility = 90)
        update(recyclerFungibility = 40, maxRecyclerFungibility = 40)

        val voucher = storedVoucher()
        assertEquals("the maximum is history and may not move", 90, voucher.maxRecyclerFungibility)
        assertEquals("the current value tracks the ring", 40, voucher.recyclerFungibility)
    }

    /**
     * The regression this test exists for. A ring already fully drained when the voucher landed in it has a
     * genuine maximum of zero, so zero must not be mistaken for "never frozen" — otherwise a later tick
     * overwrites it and the voucher appears to have had anonymity it never did.
     */
    @Test
    fun afrozenMaximumOfZeroIsStillFrozen() = runBlocking {
        givenVoucher()

        update(recyclerFungibility = 0, maxRecyclerFungibility = 0)
        update(recyclerFungibility = 70, maxRecyclerFungibility = 70)

        assertEquals("zero is a value, not an absence", 0, storedVoucher().maxRecyclerFungibility)
    }

    @Test
    fun aVoucherThatHasNeverBeenInARingHasNoMaximum() = runBlocking {
        givenVoucher()

        assertNull(storedVoucher().maxRecyclerFungibility)
    }

    @Test
    fun anUnreadableFungibilityLeavesTheStoredValuesStanding() = runBlocking {
        givenVoucher()
        update(recyclerFungibility = 55, maxRecyclerFungibility = 55)

        update(recyclerFungibility = null, maxRecyclerFungibility = null)

        val voucher = storedVoucher()
        assertEquals(55, voucher.recyclerFungibility)
        assertEquals(55, voucher.maxRecyclerFungibility)
    }

    /** A failed fungibility read must never cost the voucher the member count that decides spendability. */
    @Test
    fun anUnreadableFungibilityStillRecordsTheLocation() = runBlocking {
        givenVoucher()

        update(recyclerFungibility = null, maxRecyclerFungibility = null, recyclerMembers = 512)

        val voucher = storedVoucher()
        assertEquals(512, voucher.recyclerMembers)
        assertEquals(RING_INDEX, voucher.locationRecyclerIndex)
    }

    private fun dao() = database.recyclerVoucherDao()

    private suspend fun givenVoucher(ring: Int? = null, members: Int? = null, enteredAt: Long? = null) {
        dao().insert(voucherRow(ring, members, enteredAt))
    }

    private fun voucherRow(ring: Int?, members: Int?, enteredAt: Long?) = RecyclerVoucherLocal(
        installationId = INSTALLATION_ID,
        ringVrfKeyIndex = 0,
        ringVrfPublicKey = publicKey,
        recyclerValue = 3,
        locationRecyclerIndex = ring,
        recyclerMembers = members,
        enteredAt = enteredAt,
        recyclerFungibility = 0,
        maxRecyclerFungibility = null
    )

    private suspend fun update(
        recyclerIndex: Int = RING_INDEX,
        recyclerMembers: Int = 400,
        enteredAt: Long? = null,
        recyclerFungibility: Int? = null,
        maxRecyclerFungibility: Int? = null
    ) = dao().updateLocation(
        ringVrfPublicKey = publicKey,
        recyclerIndex = recyclerIndex,
        recyclerMembers = recyclerMembers,
        enteredAt = enteredAt,
        recyclerFungibility = recyclerFungibility,
        maxRecyclerFungibility = maxRecyclerFungibility
    )

    private suspend fun storedVoucher(): RecyclerVoucherLocal =
        dao().getByRingVrfKeyIndices(INSTALLATION_ID, listOf(0)).single()

    private companion object {
        const val RING_INDEX = 3
        val INSTALLATION_ID = ByteArray(32)
    }
}
