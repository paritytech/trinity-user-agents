package io.paritytech.polkadotapp.feature_transactions_impl.data.signer.fee

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.SubstrateKeypairFactory
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.KeyPairSigner
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.accountIdOf
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.FeeTransactionSigner
import io.novasama.substrate_sdk_android.runtime.AccountId as AccountIdRaw

class DefaultFeeSigner() : FeeTransactionSigner {
    private val fakeKeyPair = generateFakeKeyPair()

    override suspend fun fakeSignerId(chain: Chain): AccountId {
        return chain.accountIdOf(fakeKeyPair.publicKey)
    }

    override suspend fun signInheritedImplication(
        inheritedImplication: InheritedImplication,
        accountId: AccountIdRaw
    ): SignatureWrapper {
        val signer = KeyPairSigner(fakeKeyPair, multiChainEncryption())
        return signer.signInheritedImplication(inheritedImplication, accountId)
    }

    private fun multiChainEncryption() = MultiChainEncryption.Substrate(FAKE_CRYPTO_TYPE)

    private fun generateFakeKeyPair(): Keypair {
        val emptySeed = ByteArray(32) { 1 }

        return SubstrateKeypairFactory.generate(FAKE_CRYPTO_TYPE, emptySeed, junctions = emptyList())
    }
}

private val FAKE_CRYPTO_TYPE = EncryptionType.ECDSA
