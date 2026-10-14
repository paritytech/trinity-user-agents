package io.paritytech.polkadotapp.feature_chats_impl.domain.devices

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatBroadcastUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMessageSender
import io.paritytech.polkadotapp.feature_chats_api.domain.devices.BroadcastDeviceLifecycleUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.isMultiDeviceChatSupported
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

class RealBroadcastDeviceLifecycleUseCase @Inject constructor(
    private val chatBroadcastUseCase: ChatBroadcastUseCase,
    private val chatMessageSender: ChatMessageSender,
    private val accountRepository: AccountRepository,
) : BroadcastDeviceLifecycleUseCase {
    context(diagnostics: StalenessReportCollector)
    override suspend fun broadcastDeviceAdded(
        statementAccountId: AccountId,
        encryptionPublicKey: X25519PublicKey,
    ): Result<Unit> = diagnostics.markRegion(RCommon.string.chats_stall_broadcasting_device) {
        val walletAccount = accountRepository.getWalletAccount()

        chatBroadcastUseCase.broadcastToContacts(
            ChatMessage.Content.DeviceAdded(
                statementAccountId = statementAccountId,
                encryptionPublicKey = encryptionPublicKey,
            ),
            contactFilter = { it.isMultiDeviceChatSupported(walletAccount) }
        )
    }

    override suspend fun broadcastDeviceRemoved(statementAccountId: AccountId): Result<Unit> {
        val walletAccount = accountRepository.getWalletAccount()

        return chatBroadcastUseCase.broadcastToContacts(
            ChatMessage.Content.DeviceRemoved(statementAccountId = statementAccountId),
            contactFilter = { it.isMultiDeviceChatSupported(walletAccount) }
        )
    }

    override suspend fun sendDeviceAddedTo(
        contactAccountId: AccountId,
        statementAccountId: AccountId,
        encryptionPublicKey: X25519PublicKey,
    ): Result<Unit> = runCatching {
        chatMessageSender.sendUserMessage(
            chatId = ChatId.fromContact(contactAccountId),
            content = ChatMessage.Content.DeviceAdded(
                statementAccountId = statementAccountId,
                encryptionPublicKey = encryptionPublicKey,
            ),
        )
    }
}
