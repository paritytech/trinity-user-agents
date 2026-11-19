package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.signer

import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.derivation.ChatProductSubtreeProvider
import javax.inject.Inject

// X(A, B) of chat-spec RFC-0005. No period segment: the slot allocator renews the same per-chat account across days.
class ChatSignerAccountDerivation @Inject constructor(
    private val accountDerivationUseCase: AccountDerivationUseCase,
    private val chatProductSubtree: ChatProductSubtreeProvider,
) {
    suspend fun deriveKeypair(contactAccountId: AccountId): Result<Sr25519Keypair> {
        val path = derivationPath(contactAccountId)
        return accountDerivationUseCase.deriveKeypair(path).mapCatching { keypair -> keypair.requireSr25519(path) }
    }

    suspend fun deriveAccountId(contactAccountId: AccountId): Result<AccountId> {
        return deriveKeypair(contactAccountId).map { keypair -> keypair.publicKey.toDataByteArray() }
    }

    suspend fun derivationPath(contactAccountId: AccountId): String {
        val peerJunction = contactAccountId.value.toHexString(withPrefix = true)
        return "${chatProductSubtree.subtreePath()}//peer-statement//$peerJunction"
    }

    private fun Keypair.requireSr25519(path: String): Sr25519Keypair {
        return this as? Sr25519Keypair
            ?: error("Chat signer keypair at $path must be Sr25519, got ${this::class.simpleName}")
    }
}
