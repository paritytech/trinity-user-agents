package io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic

interface AccountSecretsFactory {
    fun create(
        mnemonic: Mnemonic,
        encryptionType: EncryptionType,
        derivationPath: String?
    ): MetaAccountSecrets
}
