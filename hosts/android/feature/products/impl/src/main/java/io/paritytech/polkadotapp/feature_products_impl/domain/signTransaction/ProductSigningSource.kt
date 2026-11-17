package io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.util.sign
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.sign.AccountBytesSigner
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SignVrfError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfSignature
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import io.paritytech.polkadotapp.feature_products_impl.domain.ProductAccountDerivationUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.origin.ProductAccountOrigins
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.asSignedOrigin
import io.paritytech.polkadotapp.sr25519_vrf.Sr25519VrfSigner
import io.paritytech.polkadotapp.sr25519_vrf.Sr25519VrfTranscriptItem
import javax.inject.Inject

/**
 * Resolves the signer for a single signing request. Abstracts over whether the request signs with a
 * product account or with the user's identity (wallet) account, so the signing interactors don't
 * need to know which one they're dealing with.
 */
interface ProductSigningSource {
    fun resolveAccount(): SigningAccount

    suspend fun createTransactionOrigin(): Result<TransactionOrigin>

    suspend fun signRaw(message: ByteArray, context: MessageSigningContext): Result<DataByteArray>

    /** RFC-0023: replays [transcriptLabel] + [items] into a merlin transcript and VRF-signs it. */
    suspend fun signVrf(transcriptLabel: ByteArray, items: List<VrfTranscriptItem>): Result<VrfSignature>
}

class ProductAccountSigningSource @AssistedInject constructor(
    @Assisted private val accountId: ProductAccountId,
    private val productAccountOrigins: ProductAccountOrigins,
    private val productAccountDerivationUseCase: ProductAccountDerivationUseCase,
) : ProductSigningSource {
    @AssistedFactory
    interface Factory {
        fun create(accountId: ProductAccountId): ProductAccountSigningSource
    }

    override fun resolveAccount(): SigningAccount = SigningAccount.Product(accountId)

    override suspend fun createTransactionOrigin(): Result<TransactionOrigin> {
        return productAccountOrigins.productAccountOrigin(accountId)
    }

    override suspend fun signRaw(message: ByteArray, context: MessageSigningContext): Result<DataByteArray> {
        return productAccountDerivationUseCase.deriveKeypair(accountId)
            .map { keypair -> keypair.sign(message, context).toDataByteArray() }
    }

    override suspend fun signVrf(transcriptLabel: ByteArray, items: List<VrfTranscriptItem>): Result<VrfSignature> {
        return productAccountDerivationUseCase.deriveKeypair(accountId).flatMap { keypair ->
            // schnorrkel expects the full 96-byte keypair: secret (privateKey ++ nonce) ++ publicKey.
            Sr25519VrfSigner.sign(
                keypair = keypair.privateKey + keypair.nonce + keypair.publicKey,
                transcriptLabel = transcriptLabel,
                items = items.map { item ->
                    Sr25519VrfTranscriptItem(label = item.label.value, value = item.value.value)
                },
            ).map { signature ->
                VrfSignature(
                    preOutput = signature.preOutput.toDataByteArray(),
                    proof = signature.proof.toDataByteArray(),
                )
            }
        }
    }
}

class IdentityAccountSigningSource @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountBytesSigner: AccountBytesSigner,
    private val knownChains: KnownChains,
) : ProductSigningSource {
    override fun resolveAccount(): SigningAccount = SigningAccount.IdentityAccount

    override suspend fun createTransactionOrigin(): Result<TransactionOrigin> {
        return runCatching { accountRepository.getWalletAccount().asSignedOrigin() }
    }

    override suspend fun signRaw(message: ByteArray, context: MessageSigningContext): Result<DataByteArray> {
        // chainId only selects the encryption, which is chain-independent for the sr25519 wallet account.
        return accountBytesSigner.signRawBytesByWallet(message, knownChains.people, context)
            .map { signature -> signature.signature.toDataByteArray() }
    }

    // RFC-0023 binds the VRF to a product account; the identity account has no such request shape.
    override suspend fun signVrf(transcriptLabel: ByteArray, items: List<VrfTranscriptItem>): Result<VrfSignature> {
        return Result.failure(SignVrfError.Unknown("VRF signing is not supported for the identity account"))
    }
}
