package io.paritytech.polkadotapp.feature_account_api.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import kotlinx.coroutines.flow.Flow

interface AccountRepository {
    suspend fun areAccountsInitialized(): Boolean

    fun areAccountsInitializedFlow(): Flow<Boolean>

    suspend fun initAccounts(entropy: ByteArray)

    fun walletAccountFlow(): Flow<MetaAccount>

    suspend fun getWalletAccount(): MetaAccount

    suspend fun getAccountById(id: Long): MetaAccount?

    fun subscribeAccountByPurpose(purpose: MetaAccount.Purpose): Flow<MetaAccount>

    suspend fun getAccountByPurpose(purpose: MetaAccount.Purpose): MetaAccount

    suspend fun getCandidateAlias(context: BandersnatchContext): BandersnatchAlias

    suspend fun getAliasAccount(context: BandersnatchContext): MetaAccount

    suspend fun deriveWalletAccountId(entropy: ByteArray): AccountId
}

suspend fun AccountRepository.getWalletAccountIdIn(chain: Chain): AccountId {
    return getWalletAccount().accountIdIn(chain)
}

suspend fun AccountRepository.getDepositAccount(): MetaAccount {
    return getAccountByPurpose(MetaAccount.Purpose.DEPOSIT)
}

suspend fun AccountRepository.getCandidateAccount(): MetaAccount {
    return getAccountByPurpose(MetaAccount.Purpose.CANDIDATE)
}

suspend fun AccountRepository.getAccountByIdOrThrow(id: Long): MetaAccount {
    return requireNotNull(getAccountById(id)) { "Account with id $id not found" }
}

suspend fun AccountRepository.awaitAccountsInitialized() {
    getWalletAccount()
}
