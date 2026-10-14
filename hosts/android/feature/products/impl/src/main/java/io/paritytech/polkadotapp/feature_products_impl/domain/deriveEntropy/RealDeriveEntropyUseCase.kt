package io.paritytech.polkadotapp.feature_products_impl.domain.deriveEntropy

import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_products_api.domain.deriveEntropy.DeriveEntropyUseCase
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

private val DOMAIN_SEPARATOR = "product-entropy-derivation".toByteArray(Charsets.UTF_8)
private const val MAX_KEY_SIZE = 32

class RealDeriveEntropyUseCase @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
) : DeriveEntropyUseCase {
    override suspend fun deriveEntropy(productId: ProductId, key: ByteArray): Result<ByteArray> {
        return runCatching {
            require(key.size <= MAX_KEY_SIZE) { "Key must be at most $MAX_KEY_SIZE bytes, got ${key.size}" }

            val rootEntropySource = deriveRootEntropySource().getOrThrow()
            val perProductEntropy = rootEntropySource.blake2b256(key = productId.value.encodeToByteArray().blake2b256())
            val requestedEntropy = perProductEntropy.blake2b256(key = key)

            requestedEntropy
        }
    }

    override suspend fun deriveRootEntropySource(): Result<ByteArray> = runCatching {
        val account = accountRepository.getWalletAccount()
        val mnemonic = accountSecretsStorage.requireMetaAccountPassphrase(account.id)
        mnemonic.entropy.blake2b256(key = DOMAIN_SEPARATOR)
    }
}
