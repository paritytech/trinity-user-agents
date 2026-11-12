package io.paritytech.polkadotapp.feature_statement_store_impl.domain.notificationAllocator

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins.StatementStoreOrigins
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.AllocateContext
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator.AllocateContextResolver
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableFailureKind
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.SubmissionPreparation
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxDomainId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class NotificationSlotSubmissionPolicyTest {
    private val period = 20_000u
    private val claim = DurableTxId(1)
    private val target: AccountId = byteArrayOf(0x0A).toDataByteArray()
    private val chain: Chain = mockk()
    private val context = AllocateContext(chain, listOf(PeopleCollection.People), period)
    private val extrinsic: EnrichedSendableExtrinsic = mockk()
    private val origin: TransactionOrigin = mockk()

    private val knownChains: KnownChains = mockk()
    private val contextResolver: AllocateContextResolver = mockk()
    private val seqPicker: NotificationSeqPicker = mockk()
    private val reservations = NotificationSeqReservations()
    private val origins: StatementStoreOrigins = mockk()
    private val extrinsicService: ExtrinsicService = mockk()
    private val ringUseCase: PeopleCheckMemberInRingUseCase = mockk()
    private val activeCollectionUseCase: ActivePeopleCollectionUseCase = mockk()

    private val policy = NotificationSlotSubmissionPolicy(
        knownChains, contextResolver, seqPicker, reservations, NotificationSlotAllocationLock(),
        origins, extrinsicService, ringUseCase, activeCollectionUseCase,
    )

    @Before
    fun setUp() {
        coEvery { activeCollectionUseCase.getActivePeopleCollection() } returns PeopleCollection.People
        coEvery { ringUseCase.awaitIncluded(PeopleCollection.People) } returns Result.success(Unit)
        coEvery { contextResolver.resolve() } returns Result.success(context)
    }

    @Test
    fun `gives up when no slot is free in the period`() = runBlocking<Unit> {
        withFreeSlots()

        val preparation = policy.prepareSubmission(listOf(scheduledClaim())).getOrThrow()

        assertSame(SubmissionPreparation.GiveUp, preparation[claim])
    }

    @Test
    fun `builds the claim on the slot reserved when it was scheduled`() = runBlocking<Unit> {
        reservations.reserve(target, slot(4u)).getOrThrow()
        withFreeSlots(slot(3u), slot(4u))
        withBuildableClaims()

        val preparation = policy.prepareSubmission(listOf(scheduledClaim())).getOrThrow()

        assertTrue(preparation[claim] is SubmissionPreparation.Ready)
        coVerify { origins.asResourcesNotificationSlot(period, 4u, PeopleCollection.People) }
    }

    @Test
    fun `moves the claim to a free slot once its reserved slot is taken on chain`() = runBlocking<Unit> {
        reservations.reserve(target, slot(0u)).getOrThrow()
        withFreeSlots(slot(5u))
        withBuildableClaims()

        policy.prepareSubmission(listOf(scheduledClaim())).getOrThrow()

        assertEquals(slot(5u), reservations.reservedFor(target))
        coVerify { origins.asResourcesNotificationSlot(period, 5u, PeopleCollection.People) }
    }

    @Test
    fun `giving up releases the target's reservation`() = runBlocking<Unit> {
        reservations.reserve(target, slot(0u)).getOrThrow()
        withFreeSlots()

        policy.prepareSubmission(listOf(scheduledClaim())).getOrThrow()

        assertEquals(null, reservations.reservedFor(target))
    }

    @Test
    fun `every failure kind is rebuilt`() = runBlocking<Unit> {
        DurableFailureKind.entries.forEach { kind ->
            assertTrue(policy.canRetry(mockk(relaxed = true), target, kind))
        }
    }

    private fun withFreeSlots(vararg slots: NotificationSlot) {
        coEvery { seqPicker.freeSlots(context, forTarget = target) } returns Result.success(slots.toList())
    }

    private fun withBuildableClaims() {
        every { chain.id } returns "people"
        coEvery { origins.asResourcesNotificationSlot(any(), any(), any()) } returns Result.success(origin)
        coEvery { extrinsicService.buildExtrinsic(chain, origin, any(), any()) } returns Result.success(extrinsic)
    }

    private fun scheduledClaim() = ScheduledDurableTx(
        id = claim,
        domainId = TxDomainId(NOTIFICATION_SLOT_DOMAIN_ID),
        groupId = target.notificationSlotGroup(),
        policy = target.notificationSlotPolicy(),
    )

    private fun slot(seq: UByte) = NotificationSlot(PeopleCollection.People, period, seq)
}
