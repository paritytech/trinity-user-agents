package io.paritytech.polkadotapp.feature_chats_impl.domain.deviceLifecycle

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactDevice
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactDevicesRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessageSaveProcessor
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Persists incoming [ChatMessage.Content.DeviceAdded] / [ChatMessage.Content.DeviceRemoved] /
 * [ChatMessage.Content.DeviceChatAccepted] announcements from contacts into the local device roster.
 * Outgoing copies are ignored — fan-out is the registration flow's responsibility.
 *
 * Acceptances are stored regardless of whether we still have a pending outgoing request: a contact
 * that reinstalled re-accepts our old request with a fresh device key.
 */
@Singleton
class DeviceLifecycleMessageProcessor @Inject constructor(
    private val contactDevicesRepository: ContactDevicesRepository,
) : ChatMessageSaveProcessor {
    override suspend fun onMessageSaved(message: ChatMessage) {
        val contactVariant = message.chatId.contactOrNull() ?: return
        if (message.origin !is ChatMessageOrigin.Contact) return

        val contactAccountId = contactVariant.contactAccountId

        when (val content = message.content) {
            is ChatMessage.Content.DeviceAdded -> {
                Timber.d("DeviceAdded received for contact=$contactAccountId, device=${content.statementAccountId}")
                contactDevicesRepository.addDevice(
                    ContactDevice(
                        contactAccountId = contactAccountId,
                        statementAccountId = content.statementAccountId,
                        encryptionPublicKey = content.encryptionPublicKey,
                    )
                )
            }

            is ChatMessage.Content.DeviceChatAccepted -> {
                Timber.d("DeviceChatAccepted received for contact=$contactAccountId, device=${content.device.statementAccountId}")
                contactDevicesRepository.addDevice(
                    ContactDevice(
                        contactAccountId = contactAccountId,
                        statementAccountId = content.device.statementAccountId,
                        encryptionPublicKey = content.device.encryptionPublicKey,
                    )
                )
            }

            is ChatMessage.Content.DeviceRemoved -> {
                Timber.d("DeviceRemoved received for contact=$contactAccountId, device=${content.statementAccountId}")
                contactDevicesRepository.removeDevice(
                    contactAccountId = contactAccountId,
                    statementAccountId = content.statementAccountId,
                )
            }

            else -> Unit
        }
    }
}
