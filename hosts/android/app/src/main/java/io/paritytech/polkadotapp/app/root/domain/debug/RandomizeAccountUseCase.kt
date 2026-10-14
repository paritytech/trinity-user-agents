package io.paritytech.polkadotapp.app.root.domain.debug

import io.paritytech.polkadotapp.database.dao.MetaAccountDao
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import javax.inject.Inject
import kotlin.random.Random

class RandomizeAccountUseCase @Inject constructor(
    private val accountRepository: AccountRepository,
    private val metaAccountDao: MetaAccountDao,
) {
    suspend operator fun invoke() {
        val wallet = accountRepository.getWalletAccount()
        metaAccountDao.delete(wallet.id)

        accountRepository.initAccounts(Random.nextBytes(32))
    }
}
