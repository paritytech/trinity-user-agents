package io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.AeadKey
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_revive_api.EvmAccountId
import io.paritytech.polkadotapp.feature_revive_api.toEvmAccountId
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject

class DataStoreAccount(
    val keypair: Sr25519Keypair,
    val accountId: AccountId,
    val evmAccountId: EvmAccountId,
    val encryptionKey: AeadKey,
)

interface DataStoreAccountKeys {
    suspend fun get(): Result<DataStoreAccount>
}

class RealDataStoreAccountKeys @Inject constructor(
    private val accountDerivationUseCase: AccountDerivationUseCase,
) : DataStoreAccountKeys {
    private val mutex = Mutex()
    private var derived: DataStoreAccount? = null

    override suspend fun get(): Result<DataStoreAccount> = mutex.withLock {
        derived?.let { return@withLock Result.success(it) }

        accountDerivationUseCase.deriveKeypair(DATA_STORE_DERIVATION_PATH)
            .mapCatching { keypair -> (keypair as Sr25519Keypair).toDataStoreAccount() }
            .onSuccess { derived = it }
    }

    private fun Sr25519Keypair.toDataStoreAccount(): DataStoreAccount {
        val accountId = publicKey.intoAccountId()

        return DataStoreAccount(
            keypair = this,
            accountId = accountId,
            evmAccountId = accountId.toEvmAccountId(),
            encryptionKey = deriveDataStoreEncryptionKey(privateKey + nonce),
        )
    }

    private companion object {
        const val DATA_STORE_DERIVATION_PATH = "//datastore"
    }
}

private val ENCRYPTION_CONTEXT = "encryption".encodeToByteArray()

// Keyed by the full 64-byte sr25519 secret — the scalar followed by its nonce — so iOS derives the same key from
// the same `//datastore` keypair.
fun deriveDataStoreEncryptionKey(sr25519Secret: ByteArray): AeadKey {
    return AeadKey.fromDerivedBytes(ENCRYPTION_CONTEXT.blake2b256(key = sr25519Secret))
}
