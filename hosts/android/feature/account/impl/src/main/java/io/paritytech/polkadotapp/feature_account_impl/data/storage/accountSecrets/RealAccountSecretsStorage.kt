package io.paritytech.polkadotapp.feature_account_impl.data.storage.accountSecrets

import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.novasama.substrate_sdk_android.scale.EncodableStruct
import io.novasama.substrate_sdk_android.scale.Schema
import io.novasama.substrate_sdk_android.scale.byteArray
import io.novasama.substrate_sdk_android.scale.schema
import io.novasama.substrate_sdk_android.scale.string
import io.novasama.substrate_sdk_android.scale.toHexString
import io.paritytech.polkadotapp.chains.util.KeyPairSchema
import io.paritytech.polkadotapp.chains.util.invoke
import io.paritytech.polkadotapp.chains.util.toKeypair
import io.paritytech.polkadotapp.chains.util.toStruct
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.MetaAccountSecrets
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

private const val ACCESS_SECRETS = "ACCESS_SECRETS"
private const val ADDITIONAL_KNOWN_KEYS = "ADDITIONAL_KNOWN_KEYS"
private const val ADDITIONAL_KNOWN_KEYS_DELIMITER = ","

@Singleton
class RealAccountSecretsStorage @Inject constructor(
    private val encryptedPreferences: EncryptedPreferences,
    private val dispatchers: CoroutineDispatchers,
) : AccountSecretsStorage {
    override suspend fun putMetaAccountSecrets(
        metaId: Long,
        secrets: MetaAccountSecrets,
    ) = withContext(dispatchers.io) {
        val secretsStruct = secrets.toStruct()

        encryptedPreferences.putEncryptedString(metaAccountKey(metaId, ACCESS_SECRETS), secretsStruct.toHexString())
    }

    override suspend fun getMetaAccountKeypair(metaId: Long): Keypair? = withContext(dispatchers.io) {
        encryptedPreferences.getDecryptedString(metaAccountKey(metaId, ACCESS_SECRETS))?.let { rawSecrets ->
            val secretsStruct = MetaAccountSecretsSchema.read(rawSecrets)
            val keypairStruct = secretsStruct[MetaAccountSecretsSchema.SubstrateKeypair]

            keypairStruct.toKeypair()
        }
    }

    override suspend fun getMetaAccountPassphrase(metaId: Long): Mnemonic? = withContext(dispatchers.io) {
        encryptedPreferences.getDecryptedString(metaAccountKey(metaId, ACCESS_SECRETS))?.let { rawSecrets ->
            val secretsStruct = MetaAccountSecretsSchema.read(rawSecrets)
            secretsStruct[MetaAccountSecretsSchema.Entropy]?.let { entropy ->
                MnemonicCreator.fromEntropy(entropy)
            }
        }
    }

    override suspend fun clearAllMetaAccountSecrets(metaId: Long) = withContext(dispatchers.io) {
        encryptedPreferences.removeKey(metaAccountKey(metaId, ACCESS_SECRETS))

        clearAdditionalSecrets(metaId)
    }

    override suspend fun getAdditionalMetaAccountSecret(metaId: Long, secretName: String) = withContext(Dispatchers.IO) {
        encryptedPreferences.getDecryptedString(metaAccountAdditionalKey(metaId, secretName))
    }

    override suspend fun putAdditionalMetaAccountSecret(metaId: Long, secretName: String, value: String) = withContext(Dispatchers.IO) {
        val key = metaAccountAdditionalKey(metaId, secretName)

        encryptedPreferences.putEncryptedString(key, value)
        putAdditionalSecretKeyToKnown(metaId, secretName)
    }

    private suspend fun allKnownAdditionalSecretKeys(metaId: Long): Set<String> = withContext(Dispatchers.IO) {
        val metaAccountAdditionalKnownKey = metaAccountAdditionalKnownKey(metaId)

        encryptedPreferences.getDecryptedString(metaAccountAdditionalKnownKey)
            ?.split(ADDITIONAL_KNOWN_KEYS_DELIMITER)?.toSet()
            ?: emptySet()
    }

    private suspend fun clearAdditionalSecrets(metaId: Long) {
        val allKnown = allKnownAdditionalSecretKeys(metaId)

        allKnown.forEach { secretName ->
            encryptedPreferences.removeKey(metaAccountAdditionalKey(metaId, secretName))
        }

        encryptedPreferences.removeKey(metaAccountAdditionalKnownKey(metaId))
    }

    private suspend fun putAdditionalSecretKeyToKnown(metaId: Long, secretName: String) {
        require(validAdditionalKeyName(secretName))

        val currentKnownKeys = allKnownAdditionalSecretKeys(metaId)

        val updatedKnownKeys = currentKnownKeys + secretName
        val encodedKnownKeys = updatedKnownKeys.joinToString(ADDITIONAL_KNOWN_KEYS_DELIMITER)

        encryptedPreferences.putEncryptedString(metaAccountAdditionalKnownKey(metaId), encodedKnownKeys)
    }

    private fun MetaAccountSecrets.toStruct(): EncodableStruct<MetaAccountSecretsSchema> {
        return MetaAccountSecretsSchema { secrets ->
            secrets[Entropy] = entropy
            secrets[Seed] = seed

            secrets[SubstrateKeypair] = substrateKeyPair.toStruct()
            secrets[SubstrateDerivationPath] = substrateDerivationPath
        }
    }

    private fun metaAccountKey(
        metaId: Long,
        secretName: String,
    ) = "$metaId:$secretName"

    private fun metaAccountAdditionalKnownKey(metaId: Long) = "$metaId:$ADDITIONAL_KNOWN_KEYS"

    private fun metaAccountAdditionalKey(metaId: Long, secretName: String) = "$metaId:$ADDITIONAL_KNOWN_KEYS:$secretName"

    private fun validAdditionalKeyName(secretName: String) = ADDITIONAL_KNOWN_KEYS_DELIMITER !in secretName
}

private object MetaAccountSecretsSchema : Schema<MetaAccountSecretsSchema>() {
    val Entropy by byteArray().optional()
    val Seed by byteArray().optional()

    val SubstrateKeypair by schema(KeyPairSchema)
    val SubstrateDerivationPath by string().optional()
}
