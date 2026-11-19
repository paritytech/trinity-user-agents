package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.delivery

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getAccountByIdOrThrow
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import javax.inject.Inject

class ChatRequestDeliverySigners @Inject constructor(
    private val accountRepository: AccountRepository,
    private val statementProverFactory: StatementStoreMessageProver.Factory,
) {
    suspend fun usernameSigner(contact: Contact, period: UInt): Result<ChatRequestDeliverySigner> = runCancellableCatching {
        val identityAccount = accountRepository.getAccountByIdOrThrow(contact.ourMetaAccountId)

        ChatRequestDeliverySigner(
            prover = statementProverFactory.createKeyPairProver(identityAccount),
            accountId = identityAccount.defaultPubKey(),
            delivery = ChatRequest.Delivery.DeliveredByUsername(period),
        )
    }

    fun anonymousSigner(account: ChatRequestDeliveryAccount): ChatRequestDeliverySigner {
        return ChatRequestDeliverySigner(
            prover = statementProverFactory.createKeyPairProver(account.keypair),
            accountId = account.accountId,
            delivery = ChatRequest.Delivery.DeliveredAnonymously(account.period),
        )
    }
}
