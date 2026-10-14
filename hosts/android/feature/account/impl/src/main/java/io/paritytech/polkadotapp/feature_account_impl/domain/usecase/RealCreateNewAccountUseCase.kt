package io.paritytech.polkadotapp.feature_account_impl.domain.usecase

import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.CreateNewAccountUseCase
import javax.inject.Inject

class RealCreateNewAccountUseCase @Inject constructor(
    private val accountRepository: AccountRepository
) : CreateNewAccountUseCase {
    override suspend operator fun invoke(): Result<Unit> {
        return runCatching {
            val mnemonic = MnemonicCreator.randomMnemonic(Mnemonic.Length.TWELVE)
            accountRepository.initAccounts(mnemonic.entropy)
        }
    }
}
