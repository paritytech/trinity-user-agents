package io.paritytech.polkadotapp.feature_coinage_impl.data.transaction

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.dao.CoinageAssetStateProjection
import io.paritytech.polkadotapp.database.dao.CoinageEntryDao
import io.paritytech.polkadotapp.database.model.CoinageAssetKindLocal
import io.paritytech.polkadotapp.database.model.DurableTxLocal
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageAssetState
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.installation.CoinageInstallationRepository
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.FINALIZED_SUCCESS
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus.PENDING
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * What the ledger says minted an asset it holds no entry for.
 *
 * Recovery from a previous installation's backup saves assets the finalized chain already held, under keys
 * this installation never minted — so there is no entry of ours to point at, and a null read here would mean
 * "still in flight" about a mint that settled long ago. Getting that wrong strands every payment made of such
 * a coin, so the substitution is made once, where the projection becomes domain state.
 */
class RealCoinageAssetLedgerTest {
    private val dao: CoinageEntryDao = mockk()

    private val installationRepository: CoinageInstallationRepository = mockk<CoinageInstallationRepository>().also {
        coEvery { it.getOrCreateCurrent() } returns TEST_INSTALLATION
    }

    private val ledger = RealCoinageAssetLedger(dao, installationRepository)

    @Test
    fun `an asset of a previous installation with no minter entry counts as minted`() = runTest {
        givenProjections(projection(PREVIOUS_INSTALLATION, minter = null))

        assertEquals(FINALIZED_SUCCESS, subscribedStateOf(PREVIOUS_INSTALLATION).minterStatus)
    }

    @Test
    fun `an asset of this installation with no minter entry has no minter status`() = runTest {
        givenProjections(projection(TEST_INSTALLATION, minter = null))

        assertNull(subscribedStateOf(TEST_INSTALLATION).minterStatus)
    }

    /** The substitution only ever fills a gap: a recorded mint is reported as it stands, however old the key. */
    @Test
    fun `a recorded minter of a previous installation is reported as it stands`() = runTest {
        givenProjections(projection(PREVIOUS_INSTALLATION, minter = DurableTxLocal.Status.PENDING))

        assertEquals(PENDING, subscribedStateOf(PREVIOUS_INSTALLATION).minterStatus)
    }

    @Test
    fun `a single asset read applies the same rule`() = runTest {
        val asset = coinOf(PREVIOUS_INSTALLATION)
        coEvery { dao.getAssetState(any(), any(), any()) } returns projection(PREVIOUS_INSTALLATION, minter = null)

        assertEquals(FINALIZED_SUCCESS, ledger.getAssetState(asset).getOrThrow().minterStatus)
    }

    @Test
    fun `a batched asset read applies the same rule`() = runTest {
        val asset = coinOf(PREVIOUS_INSTALLATION)
        coEvery { dao.getAssetStates(any(), any(), any()) } returns listOf(projection(PREVIOUS_INSTALLATION, minter = null))

        val states = ledger.getAssetStates(listOf(asset)).getOrThrow()

        assertEquals(FINALIZED_SUCCESS, states.getAssetStateOf(asset).minterStatus)
    }

    /** Right after a restore nothing of ours has touched the recovered assets yet, so the ledger holds no rows at all. */
    @Test
    fun `a recovered asset the ledger holds no row for counts as minted`() = runTest {
        givenProjections()

        assertEquals(FINALIZED_SUCCESS, subscribedStateOf(PREVIOUS_INSTALLATION).minterStatus)
    }

    @Test
    fun `a single asset read of a recovered asset with no row counts as minted`() = runTest {
        coEvery { dao.getAssetState(any(), any(), any()) } returns null

        assertEquals(FINALIZED_SUCCESS, ledger.getAssetState(coinOf(PREVIOUS_INSTALLATION)).getOrThrow().minterStatus)
    }

    @Test
    fun `a batched asset read of a recovered asset with no row counts as minted`() = runTest {
        val asset = coinOf(PREVIOUS_INSTALLATION)
        coEvery { dao.getAssetStates(any(), any(), any()) } returns emptyList()

        assertEquals(FINALIZED_SUCCESS, ledger.getAssetStates(listOf(asset)).getOrThrow().getAssetStateOf(asset).minterStatus)
    }

    @Test
    fun `an asset of this installation with no row is untracked`() = runTest {
        givenProjections()

        assertEquals(CoinageAssetState.UNTRACKED, subscribedStateOf(TEST_INSTALLATION))
    }

    private suspend fun subscribedStateOf(installation: CoinageInstallationId): CoinageAssetState =
        ledger.subscribeAssetStates().first().getAssetStateOf(coinOf(installation))

    private fun givenProjections(vararg projections: CoinageAssetStateProjection) {
        every { dao.subscribeAssetStates() } returns flowOf(projections.toList())
    }

    private fun coinOf(installation: CoinageInstallationId) = OwnAsset.Coin(CoinageKeyIndex(installation, DERIVATION_INDEX))

    private fun projection(installation: CoinageInstallationId, minter: DurableTxLocal.Status?) = CoinageAssetStateProjection(
        assetKind = CoinageAssetKindLocal.COIN,
        installationId = installation.value.value,
        derivationIndex = DERIVATION_INDEX,
        minterStatus = minter,
        handedOff = true,
        consumerStatus = null,
    )

    private companion object {
        const val DERIVATION_INDEX = 4

        val PREVIOUS_INSTALLATION = CoinageInstallationId(ByteArray(CoinageInstallationId.SIZE_BYTES) { 0x01 }.toDataByteArray())
    }
}
