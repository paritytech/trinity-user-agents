package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_people_api.domain.BandersnatchKeyResolver
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.NotificationSlotRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.AllocateContext
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

class NotificationSeqPickerTest {
    private val period = 20_000u
    private val target = byteArrayOf(0x01).toDataByteArray()
    private val otherTarget = byteArrayOf(0x02).toDataByteArray()
    private val chain: Chain = mockk { every { id } returns "people" }

    private val repository: NotificationSlotRepository = mockk()
    private val keyResolver: BandersnatchKeyResolver = mockk()
    private val tldProvider: DotNsTldProvider = mockk()
    private val reservations = NotificationSeqReservations()

    private val picker = NotificationSeqPicker(repository, keyResolver, tldProvider, reservations)

    @Before
    fun setUp() {
        coEvery { tldProvider.getTld() } returns Result.success(DotNsTld.parse("dot")!!)
    }

    @Test
    fun `lists People seqs before LitePeople seqs`() = runBlocking<Unit> {
        withSlots(PeopleCollection.People, highestSeq = 1u)
        withSlots(PeopleCollection.LitePeople, highestSeq = 0u)

        val free = picker.freeSlots(contextOf(PeopleCollection.LitePeople, PeopleCollection.People), forTarget = null).getOrThrow()

        assertEquals(listOf(people(0u), people(1u), lite(0u)), free)
    }

    @Test
    fun `excludes seqs registered on chain or reserved for another account`() = runBlocking<Unit> {
        withSlots(PeopleCollection.People, highestSeq = 2u, registeredSeqs = setOf(1u))
        reservations.reserve(otherTarget, people(2u)).getOrThrow()

        val free = picker.freeSlots(contextOf(PeopleCollection.People), forTarget = target).getOrThrow()

        assertEquals(listOf(people(0u)), free)
    }

    @Test
    fun `keeps a seq reserved for the asking account free for it`() = runBlocking<Unit> {
        withSlots(PeopleCollection.People, highestSeq = 1u)
        reservations.reserve(target, people(1u)).getOrThrow()

        val free = picker.freeSlots(contextOf(PeopleCollection.People), forTarget = target).getOrThrow()

        assertEquals(listOf(people(0u), people(1u)), free)
    }

    private fun withSlots(collection: PeopleCollection, highestSeq: UByte, registeredSeqs: Set<UByte> = emptySet()) {
        val aliases = (0..highestSeq.toInt()).map { aliasOf(collection, it.toUByte()) }
        coEvery { repository.highestSeqPerPeriod(collection) } returns Result.success(highestSeq)
        coEvery { keyResolver.getAliasInContext(collection, any()) } returnsMany aliases
        coEvery { repository.registeredAliases("people", match { it.toList() == aliases }) } returns
            Result.success(registeredSeqs.mapTo(mutableSetOf()) { aliasOf(collection, it) })
    }

    private fun aliasOf(collection: PeopleCollection, seq: UByte): BandersnatchAlias =
        BandersnatchAlias(byteArrayOf(collection.ordinal.toByte(), seq.toByte()))

    private fun contextOf(vararg collections: PeopleCollection) = AllocateContext(chain, collections.toList(), period)

    private fun people(seq: UByte) = NotificationSlot(PeopleCollection.People, period, seq)

    private fun lite(seq: UByte) = NotificationSlot(PeopleCollection.LitePeople, period, seq)
}
