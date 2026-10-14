package io.paritytech.polkadotapp.feature_coinage_impl.data.derivation

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519SubstrateKeypairFactory
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.SubstrateKeypairFactory
import io.novasama.substrate_sdk_android.encrypt.seed.substrate.SubstrateSeedFactory
import io.paritytech.polkadotapp.chains.util.deriveSeed32
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import javax.inject.Inject

interface CoinKeypairDerivation {
    suspend fun deriveKeypair(derivationIndex: CoinageKeyIndex): Keypair

    suspend fun deriveKeypairs(derivationIndices: List<CoinageKeyIndex>): List<Keypair>
}

class RealCoinKeypairDerivation @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
) : CoinKeypairDerivation {
    override suspend fun deriveKeypair(derivationIndex: CoinageKeyIndex): Keypair {
        val seed = rootSeed()
        val path = coinageDerivationBase(derivationIndex.installation) + itemDerivationSegment(derivationIndex.item)

        return SubstrateKeypairFactory.generate(EncryptionType.SR25519, seed, path)
    }

    override suspend fun deriveKeypairs(derivationIndices: List<CoinageKeyIndex>): List<Keypair> {
        val seed = rootSeed()

        return derivationIndices.deriveGroupedByInstallation(
            base = { installation ->
                SubstrateKeypairFactory.generate(EncryptionType.SR25519, seed, coinageDerivationBase(installation)) as Sr25519Keypair
            },
            child = { base, item ->
                val junction = SubstrateJunctionDecoder.decode(itemDerivationSegment(item)).junctions.first()
                Sr25519SubstrateKeypairFactory.deriveChild(base, junction)
            }
        )
    }

    private suspend fun rootSeed(): ByteArray {
        val accountId = accountRepository.getWalletAccount().id
        val mnemonic = accountSecretsStorage.requireMetaAccountPassphrase(accountId)

        return SubstrateSeedFactory.deriveSeed32(mnemonic.words, password = null).seed
    }

    private fun itemDerivationSegment(item: Int): String {
        return "/$item"
    }

    private fun coinageDerivationBase(installation: CoinageInstallationId): String {
        return "//coinage//${CoinageDerivationDefaults.COINAGE_MAIN_PURSE_INDEX}//${installation.asPageSegment()}"
    }
}

suspend fun CoinKeypairDerivation.getDerivedAccountId(derivationIndex: CoinageKeyIndex) = deriveKeypair(derivationIndex).publicKey.intoAccountId()

suspend fun CoinKeypairDerivation.getDerivedAccountIds(derivationIndices: List<CoinageKeyIndex>) =
    deriveKeypairs(derivationIndices).map { it.publicKey.intoAccountId() }
