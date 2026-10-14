package io.paritytech.polkadotapp.feature_account_impl.domain.usecase

import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.common.domain.model.X25519PrivateKey
import io.paritytech.polkadotapp.common.utils.X25519KeyGenerator
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.deriveKeyedEntropy
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.SharedSecretDerivationUseCase
import javax.inject.Inject

private val ECDH_ROOT_KEY = "ecdh".encodeToByteArray()

class RealSharedSecretDerivationUseCase @Inject constructor(
    private val accountSecretsStorage: AccountSecretsStorage,
    private val keyGenerator: X25519KeyGenerator,
    private val accountRepository: AccountRepository
) : SharedSecretDerivationUseCase {
    // RFC-0022: the ECDH tree hangs off the account entropy directly, not off an sr25519 derivation.
    // RFC-0004 keeps that derivation and only swaps the curve the material is interpreted under.
    override suspend fun deriveForDomain(domain: SharedSecretDerivationDomain): X25519KeyPair {
        // We only have single entropy in the app, so we can take it from any account
        val account = accountRepository.getWalletAccount()
        val mnemonic = accountSecretsStorage.requireMetaAccountPassphrase(account.id)

        val root = mnemonic.entropy.blake2b256(key = ECDH_ROOT_KEY)
        val keyMaterial = deriveKeyedEntropy(root, ecdhPath(domain))

        return keyGenerator.createKeyPair(X25519PrivateKey.fromDerivedBytes(keyMaterial))
    }

    override suspend fun generateOneTimeUse(): X25519KeyPair {
        return keyGenerator.generateRandomKeypair()
    }

    private fun ecdhPath(domain: SharedSecretDerivationDomain): String = "//${domain.domain}"
}
