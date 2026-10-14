package io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.common.domain.model.scale.toScale
import io.paritytech.polkadotapp.common.utils.decodeFromByteArrayCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.AddContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.ApplyRemoteChatMessageUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.RemoveContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatement
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.IncomingStatusScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.LocalMessageScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.LocalStatusScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.OutgoingStatusScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.ChatIdScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.DeviceStatusScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.LocalDeviceScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.SyncEntityScale
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import io.paritytech.polkadotapp.feature_sso_api.domain.model.DeviceStatus
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Instant

class SyncEntityCollector @Inject constructor(
    private val getActiveSsoSessionsUseCase: GetActiveSsoSessionsUseCase,
    private val addContactUseCase: AddContactUseCase,
    private val removeContactUseCase: RemoveContactUseCase,
    private val accountRepository: AccountRepository,
    private val applyRemoteChatMessageUseCase: ApplyRemoteChatMessageUseCase,
    private val syncFilter: DeviceSyncFilter,
) {
    suspend fun collect(checkpoint: Instant): List<SyncEntityScale> = listOfNotNull(
        collectDevices(checkpoint),
        collectChatsAdded(checkpoint),
        collectChatsRemoved(checkpoint),
        collectMessages(checkpoint),
    )

    private suspend fun collectDevices(checkpoint: Instant): SyncEntityScale.Devices? {
        val checkpointMillis = checkpoint.toEpochMilliseconds()
        val devices = getActiveSsoSessionsUseCase.getSessions()
            .filter { it.status == DeviceStatus.ACTIVE && it.lastUpdate > checkpointMillis }
        return devices.takeIf { it.isNotEmpty() }
            ?.let { SyncEntityScale.Devices(it.map(ActiveSsoSession::toLocalDeviceScale)) }
    }

    // TODO: We filter contacts from Chat With Players inside syncFilter.isContactSyncable with Contact.isMultiDeviceChatSupported
    // TODO: but we have to filter messages from this contacts also
    private suspend fun collectChatsAdded(checkpoint: Instant): SyncEntityScale.ChatsAdded? {
        val walletAccount = accountRepository.getWalletAccount()
        val added = addContactUseCase.getEstablishedContactsAddedAfter(checkpoint)
            .filter { syncFilter.isContactSyncable(it, walletAccount) }
        return added.takeIf { it.isNotEmpty() }
            ?.let { contacts -> SyncEntityScale.ChatsAdded(contacts.map { ChatIdScale.Contact(it.accountId.value) }) }
    }

    private suspend fun collectChatsRemoved(checkpoint: Instant): SyncEntityScale.ChatsRemoved? {
        val removed = removeContactUseCase.getRemovedContactsAfter(checkpoint)
        return removed.takeIf { it.isNotEmpty() }
            ?.let { ids -> SyncEntityScale.ChatsRemoved(ids.map { ChatIdScale.Contact(it.value) }) }
    }

    private suspend fun collectMessages(checkpoint: Instant): SyncEntityScale.Messages? {
        val messages = applyRemoteChatMessageUseCase.getMessagesUpdatedAfter(checkpoint).mapNotNull { syncMessage ->
            BinaryScale.decodeFromByteArrayCatching<ChatMessageStatement>(syncMessage.encoded)
                .onFailure { Timber.w(it, "SyncEntityCollector: failed to decode outbound chat message for sync") }
                .getOrNull()
                ?.let { remote ->
                    LocalMessageScale(
                        remote = remote,
                        peerId = syncMessage.peerAccountId.value,
                        status = syncMessage.toLocalStatusScale(),
                        order = syncMessage.timestamp.toULong(),
                    )
                }
        }
        Timber.d("messages to sync (${messages.size}):\n${messages.joinToString("\n") { it.logMessage() }}")
        return messages.takeIf { it.isNotEmpty() }?.let(SyncEntityScale::Messages)
    }

    private fun ApplyRemoteChatMessageUseCase.MessageForSync.toLocalStatusScale(): LocalStatusScale {
        return if (isOutgoing) {
            LocalStatusScale.Outgoing(status.toOutgoingScale())
        } else {
            LocalStatusScale.Incoming(status.toIncomingScale())
        }
    }

    private fun ChatMessage.Status.toOutgoingScale(): OutgoingStatusScale = when (this) {
        ChatMessage.Status.PROCESSING -> OutgoingStatusScale.NEW
        ChatMessage.Status.NEW,
        ChatMessage.Status.IS_SENT -> OutgoingStatusScale.SENT

        ChatMessage.Status.IS_READ -> OutgoingStatusScale.DELIVERED

        // Failed-to-deliver messages were never sent; the wire scale has no failed state, so treat as not-yet-delivered.
        ChatMessage.Status.DELIVERY_FAILED -> OutgoingStatusScale.NEW
    }

    private fun ChatMessage.Status.toIncomingScale(): IncomingStatusScale = when (this) {
        ChatMessage.Status.PROCESSING,
        ChatMessage.Status.NEW,
        ChatMessage.Status.IS_SENT,
        // DELIVERY_FAILED is only ever set on outgoing messages; map defensively for exhaustiveness.
        ChatMessage.Status.DELIVERY_FAILED -> IncomingStatusScale.NEW

        ChatMessage.Status.IS_READ -> IncomingStatusScale.SEEN
    }
}

private fun ActiveSsoSession.toLocalDeviceScale(): LocalDeviceScale {
    return LocalDeviceScale(
        statementAccountId = statementAccountId.value,
        encryptionPublicKey = encryptionPublicKey.toScale(),
        status = DeviceStatusScale.ACTIVE,
        lastUpdate = lastUpdate.toULong(),
    )
}
