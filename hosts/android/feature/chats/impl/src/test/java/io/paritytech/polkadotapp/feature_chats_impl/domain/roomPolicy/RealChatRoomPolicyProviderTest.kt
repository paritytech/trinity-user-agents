package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomDeclarations
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RealChatRoomPolicyProviderTest {
    private val repository = mockk<ChatRoomDeclarationsRepository>()
    private val provider = RealChatRoomPolicyProvider(repository)

    private val personChat = ChatId.fromContact(ByteArray(32) { 1 }.toDataByteArray())
    private val productRoom = ChatId.forExtensionRoom("ProductBot_weather", "berlin")
    private val otherProductRoom = ChatId.forExtensionRoom("ProductBot_weather", "paris")

    @Test
    fun `a person chat has no policy and the repository is not asked`() = runBlocking<Unit> {
        assertNull(provider.observePolicy(personChat).first())

        verify(exactly = 0) { repository.observe(any()) }
    }

    @Test
    fun `a product room without declarations has the default policy`() = runBlocking<Unit> {
        withDeclarations(productRoom, null)

        assertEquals(ChatRoomPolicy.Defaults, provider.observePolicy(productRoom).first())
    }

    @Test
    fun `a product room gets the policy of its declarations`() = runBlocking<Unit> {
        withDeclarations(productRoom, declarations(description = "Forecasts"))

        assertEquals("Forecasts", provider.observePolicy(productRoom).first()?.description)
    }

    @Test
    fun `the policies of a list have an entry for each product room only`() = runBlocking<Unit> {
        withDeclarations(productRoom, declarations(description = "Berlin"))
        withDeclarations(otherProductRoom, null)

        val policies = provider.observePolicies(setOf(personChat, productRoom, otherProductRoom)).first()

        assertEquals(setOf(productRoom, otherProductRoom), policies.keys)
        assertEquals("Berlin", policies.getValue(productRoom).description)
        assertEquals(ChatRoomPolicy.Defaults, policies.getValue(otherProductRoom))
    }

    @Test
    fun `a list of person chats has no policies`() = runBlocking<Unit> {
        assertEquals(emptyMap<ChatId, ChatRoomPolicy>(), provider.observePolicies(setOf(personChat)).first())
    }

    private fun withDeclarations(chatId: ChatId, declarations: ChatRoomDeclarations?) {
        every { repository.observe(chatId) } returns flowOf(declarations)
    }
}
