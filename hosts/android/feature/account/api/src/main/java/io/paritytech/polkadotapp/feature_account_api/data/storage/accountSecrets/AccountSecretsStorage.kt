package io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets

import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic

interface AccountSecretsStorage {
    suspend fun putMetaAccountSecrets(
        metaId: Long,
        secrets: MetaAccountSecrets,
    )

    suspend fun getMetaAccountKeypair(metaId: Long): Keypair?

    suspend fun getMetaAccountPassphrase(metaId: Long): Mnemonic?

    suspend fun clearAllMetaAccountSecrets(metaId: Long)

    suspend fun getAdditionalMetaAccountSecret(metaId: Long, secretName: String): String?

    suspend fun putAdditionalMetaAccountSecret(metaId: Long, secretName: String, value: String)
}

suspend fun AccountSecretsStorage.getMetaAccountKeypairOrThrow(metaId: Long): Keypair {
    return requireNotNull(getMetaAccountKeypair(metaId)) {
        "Could not get meta account keypair for metaId $metaId"
    }
}

suspend fun AccountSecretsStorage.getAdditionalMetaAccountSecretOrThrow(metaId: Long, secretName: String): String {
    return requireNotNull(getAdditionalMetaAccountSecret(metaId, secretName))
}

suspend fun AccountSecretsStorage.getMetaAccountSr25519Keypair(metaId: Long): Sr25519Keypair {
    val keypair = getMetaAccountKeypair(metaId)
    requireNotNull(keypair) {
        "Keypair for $metaId was not found"
    }

    require(keypair is Sr25519Keypair) {
        "Keypair for $metaId is not a Sr25519 keypair"
    }

    return keypair
}

suspend fun AccountSecretsStorage.requireMetaAccountPassphrase(metaId: Long): Mnemonic {
    return requireNotNull(getMetaAccountPassphrase(metaId))
}
