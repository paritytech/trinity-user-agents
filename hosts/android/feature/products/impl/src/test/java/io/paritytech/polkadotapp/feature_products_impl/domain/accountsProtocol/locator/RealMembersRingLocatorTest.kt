package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.locator

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE_LITE
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationJunction
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

private const val CANDIDATE_META_ID = 1L
private const val WALLET_META_ID = 2L
private const val CHAIN_ID = "people-chain"

class RealMembersRingLocatorTest {
    private val chainRegistry: ChainRegistry = mock(ChainRegistry::class.java)
    private val accountRepository: AccountRepository = mock(AccountRepository::class.java)

    private val locator = RealMembersRingLocator(
        chainRegistry = chainRegistry,
        fullPeopleRingLocator = FullPeopleRingLocator(accountRepository),
        litePeopleRingLocator = LitePeopleRingLocator(accountRepository),
        fallbackPeopleRingLocator = FallbackPeopleRingLocator(accountRepository),
    )

    @Test
    fun `RingNotFound when the chain genesis is unknown`() = runBlocking<Unit> {
        withUnknownChain()

        val result = locator.locateRing(ringLocation())

        assertTrue(result.isFailure)
        assertEquals(RingLocationError.RingNotFound, result.exceptionOrNull())
    }

    @Test
    fun `People collection resolves to the candidate account`() = runBlocking<Unit> {
        withResolvedChain()
        withCandidateAccount()

        val located = locator.locateRing(ringLocation(collection(RingCollectionId.PEOPLE))).getOrThrow()

        assertEquals(RingCollectionId.PEOPLE, located.collectionId)
        assertEquals(CANDIDATE_META_ID, located.metaId)
        assertEquals(CHAIN_ID, located.chainId)
    }

    @Test
    fun `Lite People collection resolves to the wallet account`() = runBlocking<Unit> {
        withResolvedChain()
        withWalletAccount()

        val located = locator.locateRing(ringLocation(collection(RingCollectionId.PEOPLE_LITE))).getOrThrow()

        assertEquals(RingCollectionId.PEOPLE_LITE, located.collectionId)
        assertEquals(WALLET_META_ID, located.metaId)
    }

    @Test
    fun `absent collection junction falls back to the People ring with candidate keys`() = runBlocking<Unit> {
        withResolvedChain()
        withCandidateAccount()

        val located = locator.locateRing(ringLocation()).getOrThrow()

        assertEquals(RingCollectionId.PEOPLE, located.collectionId)
        assertEquals(CANDIDATE_META_ID, located.metaId)
    }

    private fun ringLocation(vararg junctions: RingLocationJunction) = RingLocation(
        chainId = byteArrayOf(0x11).toDataByteArray(),
        junctions = junctions.toList(),
    )

    private fun collection(collectionId: RingCollectionId) = RingLocationJunction.CollectionId(collectionId.value)

    private suspend fun withUnknownChain() {
        whenever(chainRegistry.getChainIdByGenesisHash(any())).thenReturn(null)
    }

    private suspend fun withResolvedChain() {
        whenever(chainRegistry.getChainIdByGenesisHash(any())).thenReturn(CHAIN_ID)
    }

    private suspend fun withCandidateAccount() {
        val account = mock(MetaAccount::class.java)
        whenever(account.id).thenReturn(CANDIDATE_META_ID)
        whenever(accountRepository.getAccountByPurpose(MetaAccount.Purpose.CANDIDATE)).thenReturn(account)
    }

    private suspend fun withWalletAccount() {
        val account = mock(MetaAccount::class.java)
        whenever(account.id).thenReturn(WALLET_META_ID)
        whenever(accountRepository.getWalletAccount()).thenReturn(account)
    }
}
