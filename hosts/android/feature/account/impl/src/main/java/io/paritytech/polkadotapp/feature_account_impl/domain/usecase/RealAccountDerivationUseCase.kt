package io.paritytech.polkadotapp.feature_account_impl.domain.usecase

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsFactory
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import javax.inject.Inject

class RealAccountDerivationUseCase @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
    private val accountSecretsFactory: AccountSecretsFactory,
) : AccountDerivationUseCase {
    override suspend fun deriveAccount(derivationPath: String): Result<EncodedPublicKey> {
        return deriveKeypair(derivationPath).map { it.publicKey.toDataByteArray() }
    }

    override suspend fun deriveRootAccount(): Result<EncodedPublicKey> {
        return deriveKeypairInternal(derivationPath = null).map { it.publicKey.toDataByteArray() }
    }

    override suspend fun deriveKeypair(derivationPath: String): Result<Keypair> {
        return deriveKeypairInternal(derivationPath)
    }

    private suspend fun deriveKeypairInternal(derivationPath: String?): Result<Keypair> {
        return runCatching {
            val account = accountRepository.getWalletAccount()
            val mnemonic = accountSecretsStorage.requireMetaAccountPassphrase(account.id)

            val secrets = accountSecretsFactory.create(
                mnemonic = mnemonic,
                encryptionType = EncryptionType.SR25519,
                derivationPath = derivationPath
            )

            secrets.substrateKeyPair
        }
    }
}
