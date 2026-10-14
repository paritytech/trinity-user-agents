package io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.util.sign
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopMultiSignature
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopMultiSigner
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.slotAllocator.TransactionStorageSlotAllocator
import javax.inject.Inject

class HopSigner @Inject constructor(
    private val accountDerivation: AccountDerivationUseCase,
    private val slotAllocator: TransactionStorageSlotAllocator
) {
    suspend fun ensureCanSubmit() {
        val target = deriveKeypair().publicKey.intoAccountId()

        with(StalenessReportCollector.NoOp) {
            slotAllocator.ensureCanSubmit(target)
        }.getOrThrow()
    }

    suspend fun multiSigner(): HopMultiSigner = HopMultiSigner.SR25519(deriveKeypair().publicKey)

    suspend fun sign(payload: ByteArray): HopMultiSignature {
        val rawSignature = deriveKeypair().sign(payload, MessageSigningContext.trustedContent())
        return HopMultiSignature.SR25519(rawSignature)
    }

    private suspend fun deriveKeypair(): Sr25519Keypair {
        val keypair = accountDerivation.deriveKeypair(DERIVATION_PATH).getOrThrow()
        require(keypair is Sr25519Keypair) {
            "HOP chat key must be Sr25519, got ${keypair::class.simpleName}"
        }
        return keypair
    }

    companion object {
        private const val DERIVATION_PATH = "//allowance//bulletin//chat"
    }
}
