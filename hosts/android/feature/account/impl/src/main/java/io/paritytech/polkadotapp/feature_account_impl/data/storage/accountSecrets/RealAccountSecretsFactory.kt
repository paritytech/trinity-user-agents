package io.paritytech.polkadotapp.feature_account_impl.data.storage.accountSecrets

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder
import io.novasama.substrate_sdk_android.encrypt.junction.decode
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.SubstrateKeypairFactory
import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.novasama.substrate_sdk_android.encrypt.seed.substrate.SubstrateSeedFactory
import io.paritytech.polkadotapp.chains.util.deriveSeed32
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsFactory
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.MetaAccountSecrets
import javax.inject.Inject

class RealAccountSecretsFactory @Inject constructor() : AccountSecretsFactory {
    override fun create(
        mnemonic: Mnemonic,
        encryptionType: EncryptionType,
        derivationPath: String?
    ): MetaAccountSecrets {
        val seed = SubstrateSeedFactory.deriveSeed32(mnemonic.words, password = null)
        val junctions = SubstrateJunctionDecoder.decode(derivationPath)?.junctions.orEmpty()

        val keypair = SubstrateKeypairFactory.generate(
            encryptionType,
            seed.seed,
            junctions
        )

        return MetaAccountSecrets(
            entropy = mnemonic.entropy,
            seed = seed.seed,
            substrateDerivationPath = derivationPath,
            substrateKeyPair = keypair
        )
    }
}
