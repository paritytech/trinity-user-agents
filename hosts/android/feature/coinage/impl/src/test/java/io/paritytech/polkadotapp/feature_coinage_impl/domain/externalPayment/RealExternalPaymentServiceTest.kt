package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

import android.database.sqlite.SQLiteConstraintException
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentError
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentWorkerStarter
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.PaymentStatus
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.model.ExternalPayment
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.repository.ExternalPaymentRepository
import io.paritytech.polkadotapp.feature_coinage_impl.planks
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RealExternalPaymentServiceTest {
    private val repository: ExternalPaymentRepository = mockk()
    private val workerStarter: ExternalPaymentWorkerStarter = mockk(relaxed = true)

    private val service = RealExternalPaymentService(repository, workerStarter)

    private val key = ExternalPaymentKey(origin = "product.dot", id = "0x01")
    private val destination = byteArrayOf(7).intoAccountId()

    @Test
    fun `a new payment is registered and handed to the worker`() = runBlocking<Unit> {
        coEvery { repository.insert(any()) } returns Unit

        val result = service.initiatePayment(key, planks(10), destination)

        assertTrue(result.isSuccess)
        verify { workerStarter.start() }
    }

    /** The key is the idempotency guarantee: a second payment under it would pay twice. */
    @Test
    fun `a key that is already taken is refused`() = runBlocking<Unit> {
        coEvery { repository.insert(any()) } throws mockk<SQLiteConstraintException>(relaxed = true)

        val result = service.initiatePayment(key, planks(10), destination)

        assertTrue(result.exceptionOrNull() is ExternalPaymentError.AlreadyExists)
        verify(exactly = 0) { workerStarter.start() }
    }

    @Test
    fun `a payment still being worked on is processing`() = runBlocking<Unit> {
        givenPaymentAt(ExternalPayment.Stage.AwaitRecycling(emptyList()))

        assertEquals(PaymentStatus.Processing, service.subscribePaymentStatus(key).first())
    }

    @Test
    fun `a partially completed payment reports what reached the destination`() = runBlocking<Unit> {
        givenPaymentAt(ExternalPayment.Stage.PartiallyCompleted(planks(4)))

        assertEquals(PaymentStatus.PartiallyClaimed(planks(4)), service.subscribePaymentStatus(key).first())
    }

    @Test
    fun `status stops at the first terminal one`() = runBlocking<Unit> {
        every { repository.observe(key) } returns flowOf(
            paymentAt(ExternalPayment.Stage.EnsureVouchers),
            paymentAt(ExternalPayment.Stage.Failed("insufficient balance")),
            paymentAt(ExternalPayment.Stage.Completed),
        )

        assertEquals(
            listOf(PaymentStatus.Processing, PaymentStatus.Failed("insufficient balance")),
            service.subscribePaymentStatus(key).toList(),
        )
    }

    @Test
    fun `an unknown payment is not found`() = runBlocking<Unit> {
        every { repository.observe(key) } returns flowOf(null)

        val error = runCatching { service.subscribePaymentStatus(key).first() }.exceptionOrNull()

        assertTrue(error is ExternalPaymentError.NotFound)
    }

    private fun givenPaymentAt(stage: ExternalPayment.Stage) {
        every { repository.observe(key) } returns flowOf(paymentAt(stage))
    }

    private fun paymentAt(stage: ExternalPayment.Stage) = ExternalPayment(
        key = key,
        amount = planks(10),
        destination = destination,
        stage = stage,
        createdAt = 0,
        updatedAt = 0,
    )
}
