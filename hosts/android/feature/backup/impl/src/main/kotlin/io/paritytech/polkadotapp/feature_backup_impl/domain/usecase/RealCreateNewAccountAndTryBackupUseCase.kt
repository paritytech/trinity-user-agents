package io.paritytech.polkadotapp.feature_backup_impl.domain.usecase

import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.CreateNewAccountAndTryBackupUseCase
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import io.paritytech.polkadotapp.tools_backup_api.domain.usecase.CreateAndSaveBackupFromMnemonicUseCase
import javax.inject.Inject

class RealCreateNewAccountAndTryBackupUseCase @Inject constructor(
    private val accountRepository: AccountRepository,
    private val createBackupFromMnemonic: CreateAndSaveBackupFromMnemonicUseCase,
) : CreateNewAccountAndTryBackupUseCase {
    override suspend operator fun invoke(): Result<BackupOutcome> {
        val mnemonic = MnemonicCreator.randomMnemonic(Mnemonic.Length.TWELVE)

        return createBackupFromMnemonic(mnemonic)
            .map { BackupOutcome.Created }
            .also { accountRepository.initAccounts(mnemonic.entropy) }
            .recover { BackupOutcome.AccountsCreatedButBackupFailed }
    }
}
