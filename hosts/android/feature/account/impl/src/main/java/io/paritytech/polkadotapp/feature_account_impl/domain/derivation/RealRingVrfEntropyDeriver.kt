package io.paritytech.polkadotapp.feature_account_impl.domain.derivation

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchEntropy
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.database.dao.MetaAccountDao
import io.paritytech.polkadotapp.database.model.MetaAccountLocal
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfEntropyDeriver
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.deriveKeyedEntropy
import javax.inject.Inject
import javax.inject.Singleton

private val RING_VRF_ROOT_KEY = "ring-vrf".encodeToByteArray()

@Singleton
class RealRingVrfEntropyDeriver @Inject constructor(
    private val accountSecretsStorage: AccountSecretsStorage,
    private val metaAccountDao: MetaAccountDao,
) : RingVrfEntropyDeriver {
    override suspend fun deriveRingVrfEntropy(path: String): BandersnatchEntropy {
        // Reads the wallet account rather than taking one: every meta account shares the mnemonic,
        // so its root is the tree's root, and the DAO keeps this off the AccountRepository cycle.
        val walletMetaId = metaAccountDao.getAccountByPurpose(MetaAccountLocal.PurposeLocal.WALLET).id
        val mnemonic = accountSecretsStorage.requireMetaAccountPassphrase(walletMetaId)
        val root = mnemonic.entropy.blake2b256(key = RING_VRF_ROOT_KEY)

        return BandersnatchEntropy(deriveKeyedEntropy(root, path))
    }
}
