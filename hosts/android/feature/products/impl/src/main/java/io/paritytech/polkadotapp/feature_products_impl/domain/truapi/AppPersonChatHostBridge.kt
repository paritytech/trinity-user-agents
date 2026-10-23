package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.PersonChatHostBridge
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.externalApi
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatPayments
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.DeviceStatus
import uniffi.truapi.HostChatContact
import uniffi.truapi.HostChatPaymentReceipt
import uniffi.truapi.HostHistoryExportChunk
import uniffi.truapi.HostPairedDevice
import uniffi.truapi.HostPersonChatPaymentReceivedRequest
import java.math.BigInteger
import javax.inject.Inject

class AppPersonChatHostBridge @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val ssoSessions: GetActiveSsoSessionsUseCase,
    private val contacts: ProductChatContacts,
    private val migration: ProductChatMigration,
    private val payments: ProductChatPayments,
) : PersonChatHostBridge {
    // The native chat falls back to the Bulletin node when the registry lists no HOP node; so does the product.
    override suspend fun hopNodes(): List<String> {
        val bulletIn = chainRegistry.bulletInChain()
        val hopUrls = bulletIn.externalApi<Chain.ExternalApi.Hop>()?.urls.orEmpty()
        if (hopUrls.isNotEmpty()) return hopUrls
        return listOfNotNull(bulletIn.nodes.nodes.firstOrNull()?.unformattedUrl)
    }

    // The native chat announces a paired device by its statement account and its SSO session key (DeviceAdded).
    override suspend fun pairedDevices(): List<HostPairedDevice> =
        ssoSessions.getSessions()
            .filter { it.status == DeviceStatus.ACTIVE }
            .map { HostPairedDevice(it.statementAccountId.value, it.encryptionPublicKey.bytes.value, it.name) }

    override suspend fun setContacts(productId: String, contacts: List<HostChatContact>) {
        this.contacts.replace(productId, contacts)
    }

    override suspend fun historyExport(productId: String, offset: ULong, maxLen: UInt): HostHistoryExportChunk =
        migration.part(offset, maxLen)

    override suspend fun historyImported(productId: String) {
        migration.imported()
    }

    // The app claims the coins as the native chat claims a payment message, keyed by the message id. A payment is kept
    // also before the coinage flow runs, which picks it up when it starts, so the product never needs to ask again.
    override suspend fun paymentReceived(productId: String, payment: HostPersonChatPaymentReceivedRequest): HostChatPaymentReceipt {
        val chatId = ProductChatIdParameter(payment.roomId).toChatId(ProductId.fromStoredValue(productId).toChatExtensionId())
        payments.receive(
            chatId = chatId,
            messageId = payment.messageId,
            timestamp = payment.timestamp.toLong(),
            totalValue = Balance(BigInteger(payment.totalValue)),
            coinKeys = payment.coinKeys,
        )
        return HostChatPaymentReceipt.ACCEPTED
    }
}
