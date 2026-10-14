package io.paritytech.polkadotapp.feature_account_impl.data.storage.accountSecrets

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.database.dao.MetaAccountDao
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfDerivationProvider
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfEntropyDeriver
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_impl.data.mappers.toDomain
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

private const val BANDERSNATCH_SECRET_NAME = "BandersnatchEntropy"

@Singleton
class RealBandersnatchSecretsStorage @Inject constructor(
    private val accountSecretsStorage: AccountSecretsStorage,
    private val metaAccountDao: MetaAccountDao,
    private val ringVrfEntropyDeriver: RingVrfEntropyDeriver,
    private val ringVrfDerivationProviders: Map<MetaAccount.Purpose, @JvmSuppressWildcards RingVrfDerivationProvider>,
) : BandersnatchSecretsStorage {
    private val mutex = Mutex()

    override suspend fun getEntropy(metaId: Long): BandersnatchEntropy = mutex.withLock {
        getEntropyFromStorage(metaId) ?: generateAndSaveEntropy(metaId)
    }

    private suspend fun getEntropyFromStorage(metaId: Long): BandersnatchEntropy? {
        return accountSecretsStorage.getAdditionalMetaAccountSecret(metaId, BANDERSNATCH_SECRET_NAME)?.let {
            BandersnatchEntropy(it.fromHex())
        }
    }

    private suspend fun generateAndSaveEntropy(metaId: Long): BandersnatchEntropy {
        val metaAccount = metaAccountDao.getMetaAccount(metaId)
            ?: error("Meta account with id $metaId not found")
        val purpose = metaAccount.purpose.toDomain()

        val derivationProvider = ringVrfDerivationProviders[purpose]
            ?: error("Entropy not found and generation not supported for purpose $purpose")

        val entropy = ringVrfEntropyDeriver.deriveRingVrfEntropy(derivationProvider.provideDerivationPath())

        accountSecretsStorage.putAdditionalMetaAccountSecret(metaId, BANDERSNATCH_SECRET_NAME, entropy.value.toHexString())

        return entropy
    }
}
