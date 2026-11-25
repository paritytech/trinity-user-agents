package io.paritytech.polkadotapp.feature_statement_store_impl.data.extension

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.hash.Hasher.blake2b256
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.TransactionExtension
import io.paritytech.polkadotapp.chains.util.sign
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.common.data.substrate.model.MultiSignature
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray

class AsResourcesHolderSignatureExtension(
    private val holderKeypair: Sr25519Keypair,
) : TransactionExtension {
    override val name: String = "AsResources"

    override suspend fun implicit(): Any? = null

    override suspend fun explicit(
        inheritedImplication: InheritedImplication,
        runtimeSnapshot: RuntimeSnapshot,
    ): Any? {
        val message = inheritedImplication.encoded().blake2b256()
        val signature = holderKeypair.sign(message, MessageSigningContext.trustedContent())

        return AsResourcesInfoScale.ReplaceStatementStoreAccount(
            holderSignature = MultiSignature.Sr25519(signature.toDataByteArray()),
        ).toEncodableInstance()
    }
}
