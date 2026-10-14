package io.paritytech.polkadotapp.tools_backup_api.domain.usecase

import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic

interface CreateAndSaveBackupFromMnemonicUseCase {
    suspend operator fun invoke(mnemonic: Mnemonic): Result<Unit>
}
