package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getMetaAccountSr25519Keypair
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning.IdentitySigningGate
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.SlotPriority
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocationError
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotTag
import javax.inject.Inject

private const val SIGNER_CONNECTION_TAG = "ContactChatSigner"

internal val CHAT_SIGNER_SLOT_TAG = StatementStoreSlotTag("chat-signer")

// Only a full slot table downgrades to the username signer, and only with the user's consent for this chat;
// any other failure is returned so the caller retries.
class ContactChatSignerResolver @Inject constructor(
    private val accountDerivation: ChatSignerAccountDerivation,
    private val slotAllocator: StatementStoreSlotAllocator,
    private val accountSecretsStorage: AccountSecretsStorage,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val knownChains: KnownChains,
    private val identitySigningGate: IdentitySigningGate,
) {
    suspend fun resolve(contact: Contact): Result<ContactChatSigner> {
        val period = slotAllocator.currentPeriod()

        return accountDerivation.deriveKeypair(contact.accountId)
            .flatMap { chatKeypair -> resolveWithChatKeypair(contact, chatKeypair, period) }
            .logFailure("Failed to resolve chat signer for contact ${contact.username}")
    }

    private suspend fun usernameSigner(contact: Contact, period: UInt): Result<ContactChatSigner> = runCancellableCatching {
        val keypair = accountSecretsStorage.getMetaAccountSr25519Keypair(contact.ourMetaAccountId)
        ContactChatSigner(keypair, ChatSignerKind.USERNAME, period)
    }

    private suspend fun resolveWithChatKeypair(
        contact: Contact,
        chatKeypair: Sr25519Keypair,
        period: UInt,
    ): Result<ContactChatSigner> {
        val chatAccountId = chatKeypair.publicKey.toDataByteArray()
        val privateSigner = ensureSlot(chatAccountId).map { ContactChatSigner(chatKeypair, ChatSignerKind.PRIVATE, period) }
        return privateSigner.flatRecover { error -> usernameSignerIfNoSlot(contact, period, error) }
    }

    private suspend fun ensureSlot(chatAccountId: AccountId): Result<Unit> {
        return slotAllocator.hasCurrentAllocation(chatAccountId).flatMap { hasCurrent ->
            if (hasCurrent) return@flatMap Result.success(Unit)

            allocateSlot(chatAccountId)
        }
    }

    private suspend fun allocateSlot(chatAccountId: AccountId): Result<Unit> {
        return chainConnectionRefCounter.withConnectionEnabled(knownChains.people, SIGNER_CONNECTION_TAG) {
            with(StalenessReportCollector.NoOp) {
                slotAllocator.allocate(
                    target = chatAccountId,
                    strategy = OnExistingAllocationStrategy.IGNORE,
                    priority = SlotPriority.Critical,
                    tag = CHAT_SIGNER_SLOT_TAG,
                )
            }
        }
    }

    private suspend fun usernameSignerIfNoSlot(contact: Contact, period: UInt, error: Throwable): Result<ContactChatSigner> {
        if (error !is StatementStoreSlotAllocationError.NoAllocationAvailable) return Result.failure(error)

        return identitySigningGate.ensureConsent(contact.accountId).flatMap { usernameSigner(contact, period) }
    }
}
