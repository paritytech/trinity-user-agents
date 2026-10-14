package io.paritytech.polkadotapp.feature_transactions_impl.data.signer.submission

import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.runtime.AccountId
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.KeyPairSigner
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SubmissionTransactionSigner

class KeypairSubmissionSigner(
    keypair: Keypair,
    encryption: MultiChainEncryption
) : SubmissionTransactionSigner {
    private val delegate = KeyPairSigner(keypair, encryption)

    override suspend fun signInheritedImplication(
        inheritedImplication: InheritedImplication,
        accountId: AccountId
    ): SignatureWrapper {
        return delegate.signInheritedImplication(inheritedImplication, accountId)
    }
}
