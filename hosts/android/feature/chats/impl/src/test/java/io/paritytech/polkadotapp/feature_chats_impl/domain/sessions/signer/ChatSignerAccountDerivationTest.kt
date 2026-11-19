package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.derivation.ChatProductSubtreeProvider
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatSignerAccountDerivationTest {
    private val peerAccountId = ByteArray(32) { 0xAB.toByte() }.toDataByteArray()
    private val accountDerivationUseCase: AccountDerivationUseCase = mockk()
    private val chatProductSubtree = object : ChatProductSubtreeProvider {
        override suspend fun subtreePath(): String = "//product//chat.dot"
    }

    private val derivation = ChatSignerAccountDerivation(accountDerivationUseCase, chatProductSubtree)

    @Test
    fun `per-chat account follows the RFC-0005 path with a 0x-prefixed lowercase peer junction`() = runBlocking<Unit> {
        val path = derivation.derivationPath(peerAccountId)

        assertEquals("//product//chat.dot//peer-statement//0x${"ab".repeat(32)}", path)
    }

    @Test
    fun `keypair is derived at the per-chat path`() = runBlocking<Unit> {
        val expectedPath = "//product//chat.dot//peer-statement//0x${"ab".repeat(32)}"
        coEvery { accountDerivationUseCase.deriveKeypair(expectedPath) } returns Result.success(mockk<Sr25519Keypair>())

        derivation.deriveKeypair(peerAccountId)

        coVerify(exactly = 1) { accountDerivationUseCase.deriveKeypair(expectedPath) }
    }
}
