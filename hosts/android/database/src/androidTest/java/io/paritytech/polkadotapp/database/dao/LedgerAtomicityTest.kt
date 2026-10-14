package io.paritytech.polkadotapp.database.dao

import androidx.room.Room
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.database.AppDatabase
import io.paritytech.polkadotapp.database.model.BlockRefLocal
import io.paritytech.polkadotapp.database.model.CoinageAssetKindLocal
import io.paritytech.polkadotapp.database.model.CoinageEntryInputLocal
import io.paritytech.polkadotapp.database.model.CoinageEntryOutputLocal
import io.paritytech.polkadotapp.database.model.CoinageHandoffLocal
import io.paritytech.polkadotapp.database.model.DurableTxLocal
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The property the whole split rests on: a domain's rows and the engine's transaction row commit together
 * or not at all.
 *
 * They live in different DAOs now, so this is no longer one DAO's `@Transaction` doing the obvious thing —
 * it depends on a second DAO's writes joining a transaction the first one opened. Nothing else asserts
 * that, and if it stopped holding, a rejected registration would leave an orphaned transaction row and an
 * extrinsic could go on the wire with no record holding its inputs.
 */
@RunWith(AndroidJUnit4::class)
class LedgerAtomicityTest {

    private lateinit var database: AppDatabase
    private lateinit var txDao: DurableTxDao
    private lateinit var assetDao: CoinageEntryDao

    @Before
    fun setUp() {
        database = Room.inMemoryDatabaseBuilder(
            InstrumentationRegistry.getInstrumentation().targetContext,
            AppDatabase::class.java
        ).build()

        txDao = database.durableTxDao()
        assetDao = database.coinageEntryDao()
    }

    @After
    fun tearDown() = database.close()

    @Test
    fun aDomainsRowsJoinTheEnginesTransaction() = runBlocking<Unit> {
        txDao.withTransaction {
            val id = txDao.insert(entry())
            assetDao.insertOutputs(listOf(output(entryId = id, derivationIndex = 1)))
        }

        assertEquals(1, txDao.getAll(COINAGE).size)
        assertEquals(1, assetDao.filterMinted(listOf(keyOf(1))).size)
    }

    /** What a broken invariant does: the domain throws, and the transaction row must go with it. */
    @Test
    fun throwingFromTheDomainRollsBackTheTransactionRow() = runBlocking<Unit> {
        runCatching {
            txDao.withTransaction {
                txDao.insert(entry())
                throw IllegalStateException("invariant broken")
            }
        }

        assertTrue("an orphaned transaction row survived a rejected registration", txDao.getAll(COINAGE).isEmpty())
    }

    @Test
    fun throwingAfterWritingDomainRowsRollsBackBoth() = runBlocking<Unit> {
        runCatching {
            txDao.withTransaction {
                val id = txDao.insert(entry())
                assetDao.insertOutputs(listOf(output(entryId = id, derivationIndex = 2)))
                throw IllegalStateException("invariant broken after the domain wrote")
            }
        }

        assertTrue(txDao.getAll(COINAGE).isEmpty())
        assertTrue("a domain's rows outlived the transaction they belonged to", assetDao.filterMinted(listOf(keyOf(2))).isEmpty())
    }

    /** A handoff writes coinage rows alone, so its own transaction is what makes check-and-mark atomic. */
    @Test
    fun aDomainOnlyTransactionRollsBackToo() = runBlocking<Unit> {
        runCatching {
            assetDao.withTransaction {
                assetDao.insertHandoffs(listOf(handoff(derivationIndex = 3)))
                throw IllegalStateException("claimed after all")
            }
        }

        assertTrue(assetDao.getHandoffs().isEmpty())
    }

    /**
     * A chat payment schedules its transactions inside the transaction that saves the message, so a failure
     * anywhere in that outer transaction has to take the scheduled rows with it.
     */
    @Test
    fun aScheduleInsideAnEnclosingTransactionRollsBackWithIt() = runBlocking<Unit> {
        runCatching {
            assetDao.withTransaction {
                txDao.withTransaction {
                    val id = txDao.insert(scheduled())
                    assetDao.insertInputs(listOf(input(entryId = id, derivationIndex = 4)))
                }
                throw IllegalStateException("message could not be saved")
            }
        }

        assertTrue(txDao.getAll(COINAGE).isEmpty())
        assertTrue(assetDao.filterClaimed(listOf(keyOf(4))).isEmpty())
    }

    /** Waiting to be built holds the lock and keeps recovery alive, but gives a pass nothing to evaluate. */
    @Test
    fun aTransactionWaitingToBeBuiltIsLiveButNotDecidable() = runBlocking<Unit> {
        txDao.withTransaction {
            val id = txDao.insert(scheduled())
            assetDao.insertInputs(listOf(input(entryId = id, derivationIndex = 5)))
        }

        assertTrue(txDao.hasLiveTransactions())
        assertTrue(txDao.liveDomains().isEmpty())
        assertTrue(txDao.getAllSubmitted(COINAGE).isEmpty())
        assertEquals(1, assetDao.filterClaimed(listOf(keyOf(5))).size)
    }

    // ---- fixtures ----

    private fun entry() = DurableTxLocal(
        id = DurableTxLocal.UNSAVED_ID,
        domainId = COINAGE,
        operationGroupId = null,
        txHash = "0xtx",
        checkpoint = BlockRefLocal(blockNumber = 100, blockHash = "0xcheckpoint"),
        mortalityBlocks = 64,
        successDetectedAt = null,
        status = DurableTxLocal.Status.PENDING,
        submissionPolicyId = null,
        submissionPolicyParams = null,
    )

    private fun scheduled() = DurableTxLocal(
        id = DurableTxLocal.UNSAVED_ID,
        domainId = COINAGE,
        operationGroupId = "group",
        txHash = null,
        checkpoint = null,
        mortalityBlocks = null,
        successDetectedAt = null,
        status = DurableTxLocal.Status.PENDING_SUBMISSION,
        submissionPolicyId = "coinage-split",
        submissionPolicyParams = byteArrayOf(0),
    )

    private fun input(entryId: Long, derivationIndex: Int) = CoinageEntryInputLocal(
        entryId = entryId,
        position = 0,
        assetKind = CoinageAssetKindLocal.COIN,
        installationId = INSTALLATION,
        derivationIndex = derivationIndex,
        onChainKey = keyOf(derivationIndex),
    )

    private fun output(entryId: Long, derivationIndex: Int) = CoinageEntryOutputLocal(
        entryId = entryId,
        position = 0,
        assetKind = CoinageAssetKindLocal.COIN,
        installationId = INSTALLATION,
        derivationIndex = derivationIndex,
        onChainKey = keyOf(derivationIndex),
    )

    private fun handoff(derivationIndex: Int) = CoinageHandoffLocal(
        onChainKey = keyOf(derivationIndex),
        assetKind = CoinageAssetKindLocal.COIN,
        installationId = INSTALLATION,
        derivationIndex = derivationIndex,
        committed = false,
    )

    private fun keyOf(derivationIndex: Int) = byteArrayOf(derivationIndex.toByte())

    private companion object {
        const val COINAGE = "coinage"

        val INSTALLATION = ByteArray(32)
    }
}
