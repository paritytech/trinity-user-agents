package io.paritytech.polkadotapp.feature_chats_impl.domain.deviceLifecycle

import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactDevice
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactDevicesRepository
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.DeviceInfo
import kotlinx.coroutines.test.runTest
import org.junit.Test

/**
 * An acceptance carries the accepting device's key. It has to land in the roster even when we have no
 * pending outgoing request any more — a peer that reinstalled and re-accepts our old request is otherwise
 * unreachable, and the chat stalls.
 */
class DeviceLifecycleMessageProcessorTest {
    private val contactDevicesRepository: ContactDevicesRepository = mockk(relaxed = true)

    private val processor = DeviceLifecycleMessageProcessor(contactDevicesRepository)

    private val contactAccountId = ByteArray(32) { 1 }.intoAccountId()
    private val acceptorDevice = DeviceInfo(
        statementAccountId = ByteArray(32) { 2 }.intoAccountId(),
        encryptionPublicKey = ByteArray(32) { 3 }.requireX25519PublicKey(),
    )

    @Test
    fun `an acceptance from a contact adds the accepting device`() = runTest {
        processor.onMessageSaved(acceptance(origin = ChatMessageOrigin.Contact(contactAccountId)))

        coVerify {
            contactDevicesRepository.addDevice(
                ContactDevice(
                    contactAccountId = contactAccountId,
                    statementAccountId = acceptorDevice.statementAccountId,
                    encryptionPublicKey = acceptorDevice.encryptionPublicKey,
                )
            )
        }
    }

    @Test
    fun `our own acceptance does not touch the contact's devices`() = runTest {
        processor.onMessageSaved(acceptance(origin = ChatMessageOrigin.User))

        coVerify(exactly = 0) { contactDevicesRepository.addDevice(any()) }
    }

    private fun acceptance(origin: ChatMessageOrigin) = ChatMessage(
        id = "accept",
        chatId = ChatId.fromContact(contactAccountId),
        timestamp = 0,
        origin = origin,
        content = ChatMessage.Content.DeviceChatAccepted(requestId = "old-request", device = acceptorDevice),
        status = ChatMessage.Status.NEW,
    )
}
