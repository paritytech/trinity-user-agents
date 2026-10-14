package io.paritytech.polkadotapp.feature_transactions_impl.data.signer.submission

import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.KeyPairSigner
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.determineEncryptionOrThrow
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SubmissionTransactionSigner
import javax.inject.Inject
import javax.inject.Singleton
import io.novasama.substrate_sdk_android.runtime.AccountId as AccountIdRaw

@Singleton
class SecretsSignerFactory @Inject constructor(
    private val secretStoreV2: AccountSecretsStorage,
) {
    fun create(metaAccount: MetaAccount): SecretsSigner {
        return SecretsSigner(
            metaAccount = metaAccount,
            secretStoreV2 = secretStoreV2,
        )
    }
}

class SecretsSigner(
    private val metaAccount: MetaAccount,
    private val secretStoreV2: AccountSecretsStorage,
) : SubmissionTransactionSigner {
    override suspend fun signInheritedImplication(
        inheritedImplication: InheritedImplication,
        accountId: AccountIdRaw
    ): SignatureWrapper {
        val delegate = createDelegate(accountId)
        return delegate.signInheritedImplication(inheritedImplication, accountId)
    }

    private suspend fun createDelegate(accountId: AccountIdRaw): KeyPairSigner {
        val multiChainEncryption = metaAccount.determineEncryptionOrThrow(accountId.intoAccountId())
        val keypair = requireNotNull(secretStoreV2.getMetaAccountKeypair(metaAccount.id)) {
            "No keypair found"
        }

        return KeyPairSigner(keypair, multiChainEncryption)
    }
}
