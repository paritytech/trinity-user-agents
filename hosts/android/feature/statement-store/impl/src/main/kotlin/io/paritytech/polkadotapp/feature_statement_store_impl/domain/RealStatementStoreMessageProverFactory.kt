package io.paritytech.polkadotapp.feature_statement_store_impl.domain

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getMetaAccountSr25519Keypair
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import javax.inject.Inject

class RealStatementStoreMessageProverFactory @Inject constructor(
    private val accountSecretsStorage: AccountSecretsStorage,
) : StatementStoreMessageProver.Factory {
    override fun createKeyPairProver(metaAccount: MetaAccount): StatementStoreMessageProver {
        return KeypairSigningStatementStoreMessageProver { accountSecretsStorage.getMetaAccountSr25519Keypair(metaAccount.id) }
    }

    override fun createKeyPairProver(keypair: Sr25519Keypair): StatementStoreMessageProver {
        return KeypairSigningStatementStoreMessageProver { keypair }
    }
}
