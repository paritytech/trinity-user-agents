package io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.AddContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.ApplyRemoteChatMessageUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.RemoveContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.ChatMessageStatement
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.IncomingStatusScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.LocalStatusScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.OutgoingStatusScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.ChatIdScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.SyncEntityScale
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.SyncUpdateScale
import kotlinx.coroutines.CancellationException
import timber.log.Timber
import javax.inject.Inject

/**
 * Applies a peer's inbound [SyncUpdateScale] to local state. Inbound `Devices` are ignored —
 * our own SSO sessions list is authoritative.
 */
class SyncEntityApplier @Inject constructor(
    private val addContactUseCase: AddContactUseCase,
    private val removeContactUseCase: RemoveContactUseCase,
    private val applyRemoteChatMessageUseCase: ApplyRemoteChatMessageUseCase,
) {
    suspend fun apply(update: SyncUpdateScale) {
        for (entity in update.entities) {
            runCatching {
                when (entity) {
                    is SyncEntityScale.Devices -> Unit
                    is SyncEntityScale.ChatsAdded -> applyChatsAdded(entity)
                    is SyncEntityScale.ChatsRemoved -> applyChatsRemoved(entity, removedAt = update.timePoint.toLong())
                    is SyncEntityScale.Messages -> applyMessages(entity)
                }
            }.onFailure { error ->
                if (error is CancellationException) throw error
                Timber.w(error, "SyncEntityApplier: failed to apply entity ${entity::class.simpleName} from update id=${update.id} — skipping")
            }
        }
    }

    private suspend fun applyChatsAdded(entity: SyncEntityScale.ChatsAdded) {
        val accountIds = entity.chats
            .filterIsInstance<ChatIdScale.Contact>()
            .map { AccountId(it.accountId) }
        addContactUseCase.addAlreadyEstablishedContactsById(accountIds)
            .onFailure { Timber.w(it, "SyncEntityApplier: failed to bootstrap ${accountIds.size} contacts") }
    }

    private suspend fun applyChatsRemoved(entity: SyncEntityScale.ChatsRemoved, removedAt: Long) {
        entity.chats.forEach { chat ->
            if (chat is ChatIdScale.Contact) {
                removeContactUseCase.recordRemoteContactRemoval(
                    accountId = AccountId(chat.accountId),
                    removedAt = removedAt,
                )
            }
        }
    }

    private suspend fun applyMessages(entity: SyncEntityScale.Messages) {
        Timber.d("Apply messages:\n%s", entity.messages.joinToString("\n") { it.logMessage() })
        entity.messages.forEach { message ->
            runCatching {
                val isOutgoing = message.status is LocalStatusScale.Outgoing
                val encoded = BinaryScale.encodeToByteArray<ChatMessageStatement>(message.remote)
                applyRemoteChatMessageUseCase.apply(
                    encoded = encoded,
                    peerAccountId = AccountId(message.peerId),
                    isOutgoing = isOutgoing,
                    status = message.status.toDomainStatus(),
                )
            }.onFailure { error ->
                if (error is CancellationException) throw error
                Timber.w(error, "SyncEntityApplier: failed to apply message id=${message.remote.id} — skipping")
            }
        }
    }

    private fun LocalStatusScale.toDomainStatus(): ChatMessage.Status = when (this) {
        is LocalStatusScale.Outgoing -> when (status) {
            OutgoingStatusScale.NEW -> ChatMessage.Status.PROCESSING
            OutgoingStatusScale.SENT -> ChatMessage.Status.IS_SENT
            OutgoingStatusScale.DELIVERED -> ChatMessage.Status.IS_READ
        }

        is LocalStatusScale.Incoming -> when (status) {
            IncomingStatusScale.NEW -> ChatMessage.Status.NEW
            IncomingStatusScale.SEEN -> ChatMessage.Status.IS_READ
        }
    }
}
