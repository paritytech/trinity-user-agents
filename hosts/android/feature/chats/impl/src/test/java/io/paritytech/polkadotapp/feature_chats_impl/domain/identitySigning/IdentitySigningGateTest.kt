package io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.IdentitySigningConsentRepository
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.hours

class IdentitySigningGateTest {
    private val contactAccountId = ByteArray(32) { 3 }.toDataByteArray()
    private val consentRepository: IdentitySigningConsentRepository = mockk(relaxUnitFun = true)

    private val gate = IdentitySigningGate(consentRepository)

    @Test
    fun `granted consent lets the identity account sign`() = runBlocking<Unit> {
        withConsent(IdentitySigningConsent.GRANTED)

        assertTrue(gate.ensureConsent(contactAccountId).isSuccess)
        coVerify(exactly = 0) { consentRepository.set(any(), any()) }
    }

    @Test
    fun `unanswered consent is asked for and blocks the identity account`() = runBlocking<Unit> {
        withConsent(null)

        val error = gate.ensureConsent(contactAccountId).exceptionOrNull()

        assertTrue(error is IdentitySigningConsentRequired)
        coVerify(exactly = 1) { consentRepository.set(contactAccountId, IdentitySigningConsent.REQUESTED) }
    }

    @Test
    fun `declined consent is not asked for again`() = runBlocking<Unit> {
        withConsent(IdentitySigningConsent.DECLINED)

        val error = gate.ensureConsent(contactAccountId).exceptionOrNull()

        assertTrue(error is IdentitySigningDeclined)
        coVerify(exactly = 0) { consentRepository.set(any(), any()) }
    }

    @Test
    fun `a pending consent retry wakes up as soon as the user answers`() = runTest {
        val consent = MutableStateFlow<IdentitySigningConsent?>(IdentitySigningConsent.REQUESTED)
        every { consentRepository.subscribe(contactAccountId) } returns consent

        val retry = async { gate.awaitRetry(contactAccountId, IdentitySigningConsentRequired(contactAccountId), 1.hours) }
        yield()
        consent.value = IdentitySigningConsent.GRANTED

        retry.await()
        assertTrue(testScheduler.currentTime < 1.hours.inWholeMilliseconds)
    }

    private fun withConsent(consent: IdentitySigningConsent?) {
        coEvery { consentRepository.get(contactAccountId) } returns consent
    }
}
