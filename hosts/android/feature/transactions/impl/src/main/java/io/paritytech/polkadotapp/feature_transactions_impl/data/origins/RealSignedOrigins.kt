package io.paritytech.polkadotapp.feature_transactions_impl.data.origins

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519SubstrateKeypairFactory
import io.paritytech.polkadotapp.common.domain.model.EncodedPrivateKey
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.asSignedOrigin
import javax.inject.Inject

class RealSignedOrigins @Inject constructor(
    private val accountRepository: AccountRepository,
) : SignedOrigins {
    override suspend fun wallet(): TransactionOrigin {
        return accountRepository.getWalletAccount().asSignedOrigin()
    }

    override suspend fun candidate(): TransactionOrigin {
        return accountRepository.getCandidateAccount().asSignedOrigin()
    }

    override fun signedOrigin(metaAccount: MetaAccount): TransactionOrigin {
        return metaAccount.asSignedOrigin()
    }

    override fun signedTransactionSourceSr25519PrivateKey(privateKey: EncodedPrivateKey): Result<TransactionSignerSource.Signed> {
        return runCatching {
            val keypair = Sr25519SubstrateKeypairFactory.createKeypairFromSecret(privateKey.value)
            val encryption = MultiChainEncryption.Substrate(EncryptionType.SR25519)
            TransactionSignerSource.FromKeyPair(keypair, encryption)
        }
    }
}
