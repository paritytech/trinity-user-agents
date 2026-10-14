package io.paritytech.polkadotapp.feature_transactions_impl.data.origins

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.chains.util.KeyPairGenerator
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.TestnetTransactionOrigins
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SignedTransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import io.paritytech.polkadotapp.feature_transactions_impl.BuildConfig
import javax.inject.Inject

internal class RealTestnetTransactionOrigins @Inject constructor(
    private val environment: TestnetEnvironment,
) : TestnetTransactionOrigins {
    companion object {
        private const val TESTNET_MNEMONIC = "bottom drive obey lake curtain smoke basket hold race lonely fit walk"
        private val NIGHTLY_FUNDING_MNEMONIC = BuildConfig.NIGHTLY_FUNDING_MNEMONIC
    }

    override fun alice() = createOrigin(testnetMnemonic(), "//Alice")

    private fun nightly() = createOrigin(nightlyMnemonic())

    override fun fundingOrigin(): SignedTransactionOrigin {
        return when (environment) {
            TestnetEnvironment.TESTNET -> alice()
            TestnetEnvironment.NIGHTLY, TestnetEnvironment.PRODUCTION -> nightly()
        }
    }

    private fun createOrigin(mnemonic: Mnemonic, derivationPath: String? = null): SignedTransactionOrigin {
        val keypair = KeyPairGenerator.deriveSr25519From(mnemonic, derivationPath)
        val encryption = MultiChainEncryption.Substrate(EncryptionType.SR25519)
        val signerSource = TransactionSignerSource.FromKeyPair(keypair, encryption)

        return SignedTransactionOrigin(signerSource)
    }

    private fun testnetMnemonic(): Mnemonic {
        return MnemonicCreator.fromWords(TESTNET_MNEMONIC)
    }

    private fun nightlyMnemonic(): Mnemonic {
        check(NIGHTLY_FUNDING_MNEMONIC.isNotEmpty()) {
            "NIGHTLY_FUNDING_MNEMONIC must be configured for nightly and production testnet funding"
        }

        return MnemonicCreator.fromWords(NIGHTLY_FUNDING_MNEMONIC)
    }
}
