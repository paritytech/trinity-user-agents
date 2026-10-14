package io.paritytech.polkadotapp.database.dao

import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.model.BlockRefLocal
import io.paritytech.polkadotapp.database.model.CoinageEntryInputLocal
import io.paritytech.polkadotapp.database.model.CoinageAssetKindLocal as AssetKind
import io.paritytech.polkadotapp.database.model.DurableTxLocal
import io.paritytech.polkadotapp.database.model.DurableTxLocal.Status
import io.paritytech.polkadotapp.database.model.CoinageEntryOutputLocal
import io.paritytech.polkadotapp.database.model.CoinageHandoffLocal
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The asset-state query is the one piece of this subsystem that cannot run on the JVM, and a wrong predicate
 * in it does not fail — it silently miscounts balance. Every branch of it is asserted here against real
 * SQLite: the three sources of the key space, and each of the three facts read back off a key.
 */
@RunWith(AndroidJUnit4::class)
class CoinageEntryDaoTest {

    private lateinit var database: AppDatabase
    private lateinit var dao: CoinageEntryDao
    private lateinit var txDao: DurableTxDao

    @Before
    fun setUp() {
        database = Room.inMemoryDatabaseBuilder(
            InstrumentationRegistry.getInstrumentation().targetContext,
            AppDatabase::class.java
        ).build()

        dao = database.coinageEntryDao()
        txDao = database.durableTxDao()
    }

    @After
    fun tearDown() {
        database.close()
    }

    // ---- key space ----

    @Test
    fun assetIsKnownFromBeingMinted() = runBlocking<Unit> {
        insertEntry(status = Status.PENDING, outputs = listOf(coin(1)))

        val state = assetState(AssetKind.COIN, 1)

        assertEquals(Status.PENDING, state.minterStatus)
        assertFalse(state.handedOff)
        assertNull(state.consumerStatus)
    }

    @Test
    fun assetIsKnownFromBeingConsumed() = runBlocking<Unit> {
        insertEntry(status = Status.PENDING, inputs = listOf(coin(1)))

        val state = assetState(AssetKind.COIN, 1)

        assertNull(state.minterStatus)
        assertEquals(Status.PENDING, state.consumerStatus)
    }

    @Test
    fun assetIsKnownFromBeingHandedOff() = runBlocking<Unit> {
        dao.insertHandoffs(listOf(handoff(coin(1))))

        val state = assetState(AssetKind.COIN, 1)

        assertTrue(state.handedOff)
        assertNull(state.minterStatus)
        assertNull(state.consumerStatus)
    }

    @Test
    fun peerSentKeyNeverEntersTheKeySpace() = runBlocking<Unit> {
        val entryId = insertEntry(status = Status.PENDING)
        dao.insertInputs(listOf(peerSentInput(entryId, key = byteArrayOf(0x77))))

        assertEquals(emptyList<CoinageAssetStateProjection>(), dao.subscribeAssetStates().first())
    }

    /** An outgoing payment: we minted the coin, then handed its key over. */
    @Test
    fun aMintedAndHandedOffAssetAppearsOnce() = runBlocking<Unit> {
        val paid = coin(1)
        insertEntry(status = Status.FINALIZED_SUCCESS, outputs = listOf(paid))
        dao.insertHandoffs(listOf(handoff(paid)))

        val states = dao.subscribeAssetStates().first()

        assertEquals(1, states.size)
        assertEquals(Status.FINALIZED_SUCCESS, states.single().minterStatus)
        assertTrue(states.single().handedOff)
        assertNull(states.single().consumerStatus)
    }

    /** A split: one entry mints the coin, a later one spends it. Never combined with a handoff — the
     * Blocked-handoff invariant rejects a handed-off asset as an input. */
    @Test
    fun aMintedAndSpentAssetAppearsOnce() = runBlocking<Unit> {
        val change = coin(1)
        insertEntry(status = Status.FINALIZED_SUCCESS, outputs = listOf(change))
        insertEntry(status = Status.PENDING, inputs = listOf(change))

        val states = dao.subscribeAssetStates().first()

        assertEquals(1, states.size)
        assertEquals(Status.FINALIZED_SUCCESS, states.single().minterStatus)
        assertEquals(Status.PENDING, states.single().consumerStatus)
        assertFalse(states.single().handedOff)
    }

    // ---- consumerStatus ----

    @Test
    fun failedConsumerLeavesTheAssetUnconsumed() = runBlocking<Unit> {
        val spent = coin(1)
        insertEntry(status = Status.FINALIZED_SUCCESS, outputs = listOf(spent))
        insertEntry(status = Status.FAILURE, inputs = listOf(spent))

        val state = assetState(AssetKind.COIN, 1)

        assertNull(state.consumerStatus)
        assertEquals(Status.FINALIZED_SUCCESS, state.minterStatus)
    }

    @Test
    fun resubmissionAfterAFailureIsTheConsumerThatCounts() = runBlocking<Unit> {
        val spent = coin(1)
        insertEntry(status = Status.FAILURE, inputs = listOf(spent))
        insertEntry(status = Status.PENDING, inputs = listOf(spent))

        assertEquals(Status.PENDING, assetState(AssetKind.COIN, 1).consumerStatus)
    }

    @Test
    fun handoffOfOneAssetDoesNotMarkAnother() = runBlocking<Unit> {
        insertEntry(status = Status.PENDING, outputs = listOf(coin(1), coin(2)))
        dao.insertHandoffs(listOf(handoff(coin(1))))

        assertTrue(assetState(AssetKind.COIN, 1).handedOff)
        assertFalse(assetState(AssetKind.COIN, 2).handedOff)
    }

    @Test
    fun coinAndVoucherAtTheSameIndexAreDifferentAssets() = runBlocking<Unit> {
        insertEntry(status = Status.PENDING, outputs = listOf(coin(1)))
        insertEntry(status = Status.FAILURE, outputs = listOf(voucher(1)))

        assertEquals(Status.PENDING, assetState(AssetKind.COIN, 1).minterStatus)
        assertEquals(Status.FAILURE, assetState(AssetKind.VOUCHER, 1).minterStatus)
    }

    // ---- handoff stages ----

    @Test
    fun anUncommittedHandoffStillMarksTheAsset() = runBlocking<Unit> {
        // It has to: the keys may already be on their way, and an asset that reads free could be spent again.
        dao.insertHandoffs(listOf(handoff(coin(1), committed = false)))

        assertTrue(assetState(AssetKind.COIN, 1).handedOff)
    }

    @Test
    fun releasingUncommittedHandoffsLeavesTheCommittedOnes() = runBlocking<Unit> {
        dao.insertHandoffs(listOf(handoff(coin(1), committed = false), handoff(coin(2), committed = true)))

        dao.deleteUncommittedHandoffs()

        assertEquals(listOf(2), dao.getHandoffs().map { it.derivationIndex })
    }

    @Test
    fun committingAHandoffSurvivesTheRelease() = runBlocking<Unit> {
        val paid = coin(1)
        dao.insertHandoffs(listOf(handoff(paid, committed = false)))

        dao.commitHandoffs(listOf(paid.onChainKey))
        dao.deleteUncommittedHandoffs()

        assertTrue(assetState(AssetKind.COIN, 1).handedOff)
    }

    // ---- single-asset read ----

    @Test
    fun singleAssetReadAgreesWithTheSubscription() = runBlocking<Unit> {
        insertEntry(status = Status.PENDING_SUCCESS, outputs = listOf(coin(1), coin(2)))

        val subscribed = dao.subscribeAssetStates().first().single { it.derivationIndex == 2 }
        val read = dao.getAssetState(AssetKind.COIN, INSTALLATION, 2)!!

        assertEquals(subscribed.minterStatus, read.minterStatus)
        assertEquals(subscribed.assetKind, read.assetKind)
    }

    @Test
    fun theSameItemUnderTwoInstallationsIsTwoAssets() = runBlocking<Unit> {
        insertEntry(status = Status.FINALIZED_SUCCESS, outputs = listOf(coin(1)))
        insertEntry(status = Status.PENDING, inputs = listOf(coin(1, OTHER_INSTALLATION)))

        assertNull(dao.getAssetState(AssetKind.COIN, INSTALLATION, 1)!!.consumerStatus)
        assertEquals(Status.PENDING, dao.getAssetState(AssetKind.COIN, OTHER_INSTALLATION, 1)!!.consumerStatus)
    }

    @Test
    fun unknownAssetHasNoState() = runBlocking<Unit> {
        insertEntry(status = Status.PENDING, outputs = listOf(coin(1)))

        assertNull(dao.getAssetState(AssetKind.COIN, INSTALLATION, 9))
    }

    // ---- operation groups ----

    @Test
    fun groupEntriesAreItsOwnInRegistrationOrder() = runBlocking<Unit> {
        val first = insertEntry(status = Status.PENDING, groupId = "group")
        insertEntry(status = Status.PENDING, groupId = "other")
        val second = insertEntry(status = Status.FAILURE, groupId = "group")
        insertEntry(status = Status.PENDING, groupId = null)

        val entries = dao.subscribeGroupEntries("group").first()

        assertEquals(listOf(first, second), entries.map { it.entry.id })
        assertEquals(listOf(Status.PENDING, Status.FAILURE), entries.map { it.entry.status })
    }

    @Test
    fun groupEntriesCarryWhatEachOneMinted() = runBlocking<Unit> {
        insertEntry(status = Status.FINALIZED_SUCCESS, outputs = listOf(coin(1), coin(2)), groupId = "group")
        insertEntry(status = Status.FAILURE, outputs = listOf(coin(3)), groupId = "group")

        val entries = dao.subscribeGroupEntries("group").first()

        assertEquals(listOf(listOf(1, 2), listOf(3)), entries.map { e -> e.outputs.map { it.derivationIndex } })
    }

    // ---- liveness ----

    @Test
    fun onlyUndecidedEntriesCountAsLive() = runBlocking<Unit> {
        assertFalse(txDao.hasLiveTransactions())

        insertEntry(status = Status.FINALIZED_SUCCESS)
        insertEntry(status = Status.FAILURE)
        assertFalse(txDao.hasLiveTransactions())

        insertEntry(status = Status.PENDING_SUCCESS)
        assertTrue(txDao.hasLiveTransactions())
    }

    // ---- fixtures ----

    private suspend fun assetState(kind: AssetKind, derivationIndex: Int): CoinageAssetStateProjection =
        dao.subscribeAssetStates().first().single {
            it.assetKind == kind && it.derivationIndex == derivationIndex
        }

    private var nextTxHash = 0

    private suspend fun insertEntry(
        status: Status,
        inputs: List<Asset> = emptyList(),
        outputs: List<Asset> = emptyList(),
        groupId: String? = null,
    ): Long {
        val id = txDao.insert(
            DurableTxLocal(
                id = DurableTxLocal.UNSAVED_ID,
                domainId = "coinage",
                operationGroupId = groupId,
                txHash = "0x${nextTxHash++}",
                checkpoint = BlockRefLocal(blockNumber = 100, blockHash = "0xcheckpoint"),
                mortalityBlocks = 64,
                successDetectedAt = null,
                status = status,
                submissionPolicyId = null,
                submissionPolicyParams = null,
            )
        )

        dao.insertInputs(
            inputs.mapIndexed { position, asset ->
                CoinageEntryInputLocal(
                    entryId = id,
                    position = position,
                    assetKind = asset.kind,
                    installationId = asset.installation,
                    derivationIndex = asset.derivationIndex,
                    onChainKey = asset.onChainKey,
                )
            }
        )
        dao.insertOutputs(
            outputs.mapIndexed { position, asset ->
                CoinageEntryOutputLocal(
                    entryId = id,
                    position = position,
                    assetKind = asset.kind,
                    installationId = asset.installation,
                    derivationIndex = asset.derivationIndex,
                    onChainKey = asset.onChainKey,
                )
            }
        )

        return id
    }

    private fun peerSentInput(entryId: Long, key: ByteArray) = CoinageEntryInputLocal(
        entryId = entryId,
        position = 0,
        assetKind = AssetKind.COIN,
        installationId = null,
        derivationIndex = null,
        onChainKey = key,
    )

    private fun handoff(asset: Asset, committed: Boolean = true) = CoinageHandoffLocal(
        onChainKey = asset.onChainKey,
        assetKind = asset.kind,
        installationId = asset.installation,
        derivationIndex = asset.derivationIndex,
        committed = committed,
    )

    private class Asset(val kind: AssetKind, val derivationIndex: Int, val installation: ByteArray) {
        val onChainKey = byteArrayOf(kind.ordinal.toByte(), derivationIndex.toByte(), installation.first())
    }

    private fun coin(derivationIndex: Int, installation: ByteArray = INSTALLATION) = Asset(AssetKind.COIN, derivationIndex, installation)

    private fun voucher(derivationIndex: Int) = Asset(AssetKind.VOUCHER, derivationIndex, INSTALLATION)

    private companion object {
        val INSTALLATION = ByteArray(32) { 1 }
        val OTHER_INSTALLATION = ByteArray(32) { 2 }
    }
}
