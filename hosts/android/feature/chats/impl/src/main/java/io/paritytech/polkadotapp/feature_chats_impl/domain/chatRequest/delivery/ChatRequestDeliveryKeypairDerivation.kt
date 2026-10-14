package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import javax.inject.Inject

/** A statement account used for one chat request in one period, so nothing on chain links it to us or to another. */
class ChatRequestDeliveryAccount(
    val keypair: Sr25519Keypair,
    val period: UInt,
) {
    val accountId: AccountId get() = keypair.publicKey.toDataByteArray()
}

class ChatRequestDeliveryKeypairDerivation @Inject constructor(
    private val accountDerivationUseCase: AccountDerivationUseCase,
) {
    suspend fun deliveryAccount(requestId: ChatRequestId, period: UInt): Result<ChatRequestDeliveryAccount> {
        return accountDerivationUseCase.deriveKeypair("//chat-request//$requestId//$period").mapCatching { keypair ->
            val sr25519 = keypair as? Sr25519Keypair
                ?: error("Chat request delivery keypair must be Sr25519, got ${keypair::class.simpleName} for request $requestId")
            ChatRequestDeliveryAccount(sr25519, period)
        }
    }
}
