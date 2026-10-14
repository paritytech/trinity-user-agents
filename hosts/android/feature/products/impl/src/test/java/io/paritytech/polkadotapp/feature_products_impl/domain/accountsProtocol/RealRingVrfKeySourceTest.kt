package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfEntropyDeriver
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_members_api.domain.model.MemberSource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.LocatedRing
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.MembersRingLocator
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocation
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.RingLocationJunction
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry.ReservedRingVrfKeys
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry.RingVrfKeyRegistry
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verifyNoInteractions

private val PEOPLE_RING = ringOf("pop:polkadot.network/people")
private val OTHER_RING = ringOf("pop:polkadot.network/people-lite")

private val OWN_HANDLE = ProductAccountId("game.dot", DerivationIndex32.fromUInt(7u))
private val PERSONHOOD_HANDLE = ProductAccountId("peopl.dot", DerivationIndex32.fromUInt(0u))
private val UNKNOWN_HANDLE = ProductAccountId("stranger.dot", DerivationIndex32.fromUInt(3u))

private fun ringOf(collection: String) = RingLocation(
    chainId = ByteArray(32) { 1 }.toDataByteArray(),
    junctions = listOf(
        RingLocationJunction.PalletInstance(9u),
        RingLocationJunction.CollectionId(RingCollectionId.paddedString(collection).value),
    )
)

class RealRingVrfKeySourceTest {
    private val registry: RingVrfKeyRegistry = mock(RingVrfKeyRegistry::class.java)
    private val reservedRingVrfKeys: ReservedRingVrfKeys = mock(ReservedRingVrfKeys::class.java)
    private val ringVrfEntropyDeriver = FakeRingVrfEntropyDeriver()
    private val membersRingLocator: MembersRingLocator = mock(MembersRingLocator::class.java)

    private val keySource = RealRingVrfKeySource(
        registry = registry,
        reservedRingVrfKeys = reservedRingVrfKeys,
        ringVrfEntropyDeriver = ringVrfEntropyDeriver,
        membersRingLocator = membersRingLocator,
    )

    @Test
    fun `registered handle resolves to entropy derived at its own path`() = runBlocking<Unit> {
        whenever(registry.declaredRings(OWN_HANDLE)).thenReturn(listOf(PEOPLE_RING))

        val member = keySource.resolveMember(OWN_HANDLE, PEOPLE_RING).getOrThrow()

        assertTrue(member is MemberSource.Entropy)
        assertEquals(
            "//game.dot//${OWN_HANDLE.index.asPathSegment()}",
            ringVrfEntropyDeriver.lastPath,
        )
    }

    @Test
    fun `a ring the handle never declared is rejected`() = runBlocking<Unit> {
        whenever(registry.declaredRings(OWN_HANDLE)).thenReturn(listOf(PEOPLE_RING))

        val error = keySource.resolveMember(OWN_HANDLE, OTHER_RING).exceptionOrNull()

        assertEquals(RingVrfKeyError.KeyNotInRing, error)
    }

    @Test
    fun `an unregistered reserved handle falls back to the embedded locator`() = runBlocking<Unit> {
        whenever(registry.declaredRings(PERSONHOOD_HANDLE)).thenReturn(emptyList())
        whenever(reservedRingVrfKeys.isReserved(PERSONHOOD_HANDLE)).thenReturn(true)
        whenever(membersRingLocator.locateRing(PEOPLE_RING)).thenReturn(
            Result.success(LocatedRing("people", RingCollectionId.paddedString("pop:polkadot.network/people"), metaId = 42L))
        )

        val member = keySource.resolveMember(PERSONHOOD_HANDLE, PEOPLE_RING).getOrThrow()

        assertTrue(member is MemberSource.Account)
        assertEquals(42L, (member as MemberSource.Account).metaId)
    }

    @Test
    fun `an unknown handle is not servable`() = runBlocking<Unit> {
        whenever(registry.declaredRings(UNKNOWN_HANDLE)).thenReturn(emptyList())
        whenever(reservedRingVrfKeys.isReserved(UNKNOWN_HANDLE)).thenReturn(false)

        val error = keySource.resolveMember(UNKNOWN_HANDLE, PEOPLE_RING).exceptionOrNull()

        assertEquals(RingVrfKeyError.KeyNotRegistered, error)
    }

    @Test
    fun `signing needs no ring but still needs a known handle`() = runBlocking<Unit> {
        whenever(registry.declaredRings(UNKNOWN_HANDLE)).thenReturn(emptyList())
        whenever(reservedRingVrfKeys.isReserved(UNKNOWN_HANDLE)).thenReturn(false)

        val error = keySource.resolveEntropy(UNKNOWN_HANDLE).exceptionOrNull()

        assertEquals(RingVrfKeyError.KeyNotRegistered, error)
    }

    @Test
    fun `signing derives entropy for a reserved handle without consulting a ring`() = runBlocking<Unit> {
        whenever(registry.declaredRings(PERSONHOOD_HANDLE)).thenReturn(emptyList())
        whenever(reservedRingVrfKeys.isReserved(PERSONHOOD_HANDLE)).thenReturn(true)

        keySource.resolveEntropy(PERSONHOOD_HANDLE).getOrThrow()

        assertEquals("//peopl.dot//${PERSONHOOD_HANDLE.index.asPathSegment()}", ringVrfEntropyDeriver.lastPath)
        verifyNoInteractions(membersRingLocator)
    }
}

private class FakeRingVrfEntropyDeriver : RingVrfEntropyDeriver {
    var lastPath: String? = null
        private set

    override suspend fun deriveRingVrfEntropy(path: String): BandersnatchEntropy {
        lastPath = path
        return BandersnatchEntropy(ByteArray(32) { 5 })
    }
}
