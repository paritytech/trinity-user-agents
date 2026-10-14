package io.paritytech.polkadotapp.tools_backup_impl.domain.usecase

import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.util.KeyPairGenerator
import io.paritytech.polkadotapp.chains.util.addressOf
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import io.paritytech.polkadotapp.tools_backup_api.domain.model.Backup
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupMetadata
import io.paritytech.polkadotapp.tools_backup_api.domain.usecase.CreateAndSaveBackupFromMnemonicUseCase
import javax.inject.Inject

class RealCreateAndSaveBackupFromMnemonicUseCase @Inject constructor(
    private val backupService: BackupService,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains
) : CreateAndSaveBackupFromMnemonicUseCase {
    override suspend operator fun invoke(mnemonic: Mnemonic): Result<Unit> {
        val backup = Backup(mnemonic.entropy)
        val accountId = KeyPairGenerator
            .deriveSr25519From(mnemonic)
            .publicKey
            .intoAccountId()
        val address = chainRegistry.getChain(knownChains.people).addressOf(accountId)

        return backupService.saveBackup(backup, BackupMetadata(address))
    }
}
