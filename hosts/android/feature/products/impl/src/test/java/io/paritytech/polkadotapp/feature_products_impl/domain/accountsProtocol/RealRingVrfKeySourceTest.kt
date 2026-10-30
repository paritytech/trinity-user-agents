package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol

import io.mockk.Called
import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.verify
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
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

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
    private val registry: RingVrfKeyRegistry = mockk()
    private val reservedRingVrfKeys: ReservedRingVrfKeys = mockk()
    private val ringVrfEntropyDeriver = FakeRingVrfEntropyDeriver()
    private val membersRingLocator: MembersRingLocator = mockk()

    private val keySource = RealRingVrfKeySource(
        registry = registry,
        reservedRingVrfKeys = reservedRingVrfKeys,
        ringVrfEntropyDeriver = ringVrfEntropyDeriver,
        membersRingLocator = membersRingLocator,
    )

    @Test
    fun `registered handle resolves to entropy derived at its own path`() = runBlocking<Unit> {
        coEvery { registry.declaredRings(OWN_HANDLE) } returns listOf(PEOPLE_RING)

        val member = keySource.resolveMember(OWN_HANDLE, PEOPLE_RING).getOrThrow()

        assertTrue(member is MemberSource.Entropy)
        assertEquals(
            "//game.dot//${OWN_HANDLE.index.asPathSegment()}",
            ringVrfEntropyDeriver.lastPath,
        )
    }

    @Test
    fun `a ring the handle never declared is rejected`() = runBlocking<Unit> {
        coEvery { registry.declaredRings(OWN_HANDLE) } returns listOf(PEOPLE_RING)

        val error = keySource.resolveMember(OWN_HANDLE, OTHER_RING).exceptionOrNull()

        assertEquals(RingVrfKeyError.KeyNotInRing, error)
    }

    @Test
    fun `an unregistered reserved handle falls back to the embedded locator`() = runBlocking<Unit> {
        coEvery { registry.declaredRings(PERSONHOOD_HANDLE) } returns emptyList()
        coEvery { reservedRingVrfKeys.isReserved(PERSONHOOD_HANDLE) } returns true
        coEvery { membersRingLocator.locateRing(PEOPLE_RING) } returns
            Result.success(LocatedRing("people", RingCollectionId.paddedString("pop:polkadot.network/people"), metaId = 42L))

        val member = keySource.resolveMember(PERSONHOOD_HANDLE, PEOPLE_RING).getOrThrow()

        assertTrue(member is MemberSource.Account)
        assertEquals(42L, (member as MemberSource.Account).metaId)
    }

    @Test
    fun `an unknown handle is not servable`() = runBlocking<Unit> {
        coEvery { registry.declaredRings(UNKNOWN_HANDLE) } returns emptyList()
        coEvery { reservedRingVrfKeys.isReserved(UNKNOWN_HANDLE) } returns false

        val error = keySource.resolveMember(UNKNOWN_HANDLE, PEOPLE_RING).exceptionOrNull()

        assertEquals(RingVrfKeyError.KeyNotRegistered, error)
    }

    @Test
    fun `signing needs no ring but still needs a known handle`() = runBlocking<Unit> {
        coEvery { registry.declaredRings(UNKNOWN_HANDLE) } returns emptyList()
        coEvery { reservedRingVrfKeys.isReserved(UNKNOWN_HANDLE) } returns false

        val error = keySource.resolveEntropy(UNKNOWN_HANDLE).exceptionOrNull()

        assertEquals(RingVrfKeyError.KeyNotRegistered, error)
    }

    @Test
    fun `signing derives entropy for a reserved handle without consulting a ring`() = runBlocking<Unit> {
        coEvery { registry.declaredRings(PERSONHOOD_HANDLE) } returns emptyList()
        coEvery { reservedRingVrfKeys.isReserved(PERSONHOOD_HANDLE) } returns true

        keySource.resolveEntropy(PERSONHOOD_HANDLE).getOrThrow()

        assertEquals("//peopl.dot//${PERSONHOOD_HANDLE.index.asPathSegment()}", ringVrfEntropyDeriver.lastPath)
        verify { membersRingLocator wasNot Called }
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
