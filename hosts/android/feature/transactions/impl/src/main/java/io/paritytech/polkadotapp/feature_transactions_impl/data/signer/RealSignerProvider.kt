package io.paritytech.polkadotapp.feature_transactions_impl.data.signer

import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_transactions.api.data.SignerProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.FeeTransactionSigner
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SubmissionTransactionSigner
import io.paritytech.polkadotapp.feature_transactions_impl.data.signer.fee.DefaultFeeSigner
import io.paritytech.polkadotapp.feature_transactions_impl.data.signer.submission.KeypairSubmissionSigner
import io.paritytech.polkadotapp.feature_transactions_impl.data.signer.submission.SecretsSignerFactory
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealSignerProvider @Inject constructor(
    private val secretsSignerFactory: SecretsSignerFactory,
) : SignerProvider {
    override suspend fun submissionSigner(metaAccount: MetaAccount): SubmissionTransactionSigner {
        return when (metaAccount.signerType) {
            MetaAccount.SignerType.SECRETS -> secretsSignerFactory.create(metaAccount)
        }
    }

    override suspend fun submissionSigner(keypair: Keypair, encryption: MultiChainEncryption): SubmissionTransactionSigner {
        return KeypairSubmissionSigner(keypair, encryption)
    }

    override suspend fun feeSigner(metaAccount: MetaAccount): FeeTransactionSigner {
        return when (metaAccount.signerType) {
            MetaAccount.SignerType.SECRETS -> DefaultFeeSigner()
        }
    }

    override suspend fun feeSigner(keypair: Keypair, encryption: MultiChainEncryption): FeeTransactionSigner {
        return DefaultFeeSigner()
    }
}
