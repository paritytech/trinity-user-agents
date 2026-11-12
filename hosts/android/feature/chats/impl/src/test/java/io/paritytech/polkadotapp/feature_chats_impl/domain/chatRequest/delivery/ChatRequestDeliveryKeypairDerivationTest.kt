package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class ChatRequestDeliveryKeypairDerivationTest {
    private val accountDerivationUseCase: AccountDerivationUseCase = mockk()
    private val derivation = ChatRequestDeliveryKeypairDerivation(accountDerivationUseCase)

    @Before
    fun setUp() {
        coEvery { accountDerivationUseCase.deriveKeypair(any()) } returns Result.success(mockk<Sr25519Keypair>())
    }

    @Test
    fun `request account is T_req of RFC-0005`() = runBlocking<Unit> {
        val account = derivation.deliveryAccount("request-id", 20_000u).getOrThrow()

        coVerify(exactly = 1) { accountDerivationUseCase.deriveKeypair("//chat-request//request-id//20000") }
        assertEquals(20_000u, account.period)
    }

    @Test
    fun `a request id that would add junctions is refused before deriving`() = runBlocking<Unit> {
        val error = derivation.acceptanceAccount("request//injected", 20_000u).exceptionOrNull()

        assertTrue(error is InvalidRequestIdJunction)
        coVerify(exactly = 0) { accountDerivationUseCase.deriveKeypair(any()) }
    }

    @Test
    fun `an empty request id is refused`() = runBlocking<Unit> {
        assertTrue(derivation.deliveryAccount("", 20_000u).exceptionOrNull() is InvalidRequestIdJunction)
    }

    @Test
    fun `acceptance account is T_acc of RFC-0005`() = runBlocking<Unit> {
        val account = derivation.acceptanceAccount("request-id", 20_000u).getOrThrow()

        coVerify(exactly = 1) { accountDerivationUseCase.deriveKeypair("//chat-accept//request-id//20000") }
        assertEquals(20_000u, account.period)
    }
}
