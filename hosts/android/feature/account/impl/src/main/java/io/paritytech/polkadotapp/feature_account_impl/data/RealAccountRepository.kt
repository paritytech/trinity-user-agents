package io.paritytech.polkadotapp.feature_account_impl.data

import io.novasama.substrate_sdk_android.encrypt.junction.JunctionDecoder
import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.substrateAccountId
import io.paritytech.polkadotapp.database.dao.MetaAccountDao
import io.paritytech.polkadotapp.database.model.MetaAccountLocal
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsFactory
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getAliasInContext
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.AccountDerivationProvider
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.SubstrateCryptoType
import io.paritytech.polkadotapp.feature_account_api.domain.model.toEncryption
import io.paritytech.polkadotapp.feature_account_impl.data.mappers.toDomain
import io.paritytech.polkadotapp.feature_account_impl.data.mappers.toLocal
import kotlinx.coroutines.DelicateCoroutinesApi
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealAccountRepository @Inject constructor(
    private val metaAccountDao: MetaAccountDao,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val accountSecretsStorage: AccountSecretsStorage,
    private val accountSecretsFactory: AccountSecretsFactory,
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
    private val accountDerivationProviders: Map<MetaAccount.Purpose, @JvmSuppressWildcards AccountDerivationProvider>,
) : AccountRepository {
    companion object {
        private val DEFAULT_SUBSTRATE_CRYPTO_TYPE = SubstrateCryptoType.SR25519
    }

    private val accountAliasMutex = Mutex()

    @OptIn(DelicateCoroutinesApi::class)
    private val walletAccountFlow = metaAccountDao.walletAccountFlow()
        .filterNotNull()
        .map(MetaAccountLocal::toDomain)
        .inBackground()
        .shareIn(GlobalScope, started = SharingStarted.Eagerly, replay = 1)

    override suspend fun getWalletAccount(): MetaAccount {
        return walletAccountFlow.first()
    }

    override suspend fun getAccountById(id: Long): MetaAccount? {
        return metaAccountDao.getMetaAccount(id)?.toDomain()
    }

    override fun subscribeAccountByPurpose(purpose: MetaAccount.Purpose): Flow<MetaAccount> {
        return metaAccountDao
            .subscribeAccountByPurpose(purpose.toLocal())
            .filterNotNull()
            .map { it.toDomain() }
    }

    override suspend fun getAccountByPurpose(purpose: MetaAccount.Purpose): MetaAccount {
        awaitAccountsInitialized()

        return metaAccountDao.getAccountByPurpose(purpose.toLocal())
            .toDomain()
    }

    override suspend fun areAccountsInitialized(): Boolean = metaAccountDao.isAnyMetaAccountExists()

    override fun areAccountsInitializedFlow(): Flow<Boolean> = metaAccountDao.isAnyMetaAccountExistsFlow()

    override suspend fun initAccounts(entropy: ByteArray) = withContext(coroutineDispatchers.io) {
        val mnemonic = MnemonicCreator.fromEntropy(entropy)

        val derivationPaths = MetaAccount.Purpose.entries
            .filter { it != MetaAccount.Purpose.ALIAS }
            .associateWith { it.derivationPath() }

        metaAccountDao.withTransaction {
            derivationPaths.forEach { (purpose, derivationPath) ->
                createMetaAccount(
                    mnemonic = mnemonic,
                    purpose = purpose,
                    aliasContext = null,
                    derivationPath = derivationPath
                )
            }
        }
    }

    override fun walletAccountFlow(): Flow<MetaAccount> {
        return walletAccountFlow
    }

    private suspend fun createMetaAccount(
        purpose: MetaAccount.Purpose,
        mnemonic: Mnemonic,
        aliasContext: BandersnatchContext?,
        derivationPath: String
    ): Long {
        val secrets = accountSecretsFactory.create(
            mnemonic = mnemonic,
            encryptionType = DEFAULT_SUBSTRATE_CRYPTO_TYPE.toEncryption(),
            derivationPath = derivationPath
        )

        val metaAccountLocal = MetaAccountLocal(
            substratePublicKey = secrets.substrateKeyPair.publicKey,
            substrateCryptoType = DEFAULT_SUBSTRATE_CRYPTO_TYPE.toLocal(),
            substrateAccountId = secrets.substrateKeyPair.publicKey.substrateAccountId().value,
            name = purpose.name,
            signerType = MetaAccountLocal.SignerTypeLocal.SECRETS,
            purpose = purpose.toLocal(),
            aliasContext = aliasContext?.value
        )

        val metaId = metaAccountDao.insertMetaAccount(metaAccountLocal)
        accountSecretsStorage.putMetaAccountSecrets(metaId, secrets)

        return metaId
    }

    override suspend fun getCandidateAlias(context: BandersnatchContext): BandersnatchAlias {
        return bandersnatchSecretsStorage.getAliasInContext(getCandidateAccount().id, context)
    }

    override suspend fun getAliasAccount(context: BandersnatchContext): MetaAccount = accountAliasMutex.withLock {
        val existingAlias = metaAccountDao.getAliasAccount(context.value)?.toDomain()

        if (existingAlias != null) return existingAlias

        val mnemonic = accountSecretsStorage.requireMetaAccountPassphrase(walletAccountFlow.first().id)
        createMetaAccount(
            mnemonic = mnemonic,
            purpose = MetaAccount.Purpose.ALIAS,
            aliasContext = context,
            derivationPath = context.aliasAccountDerivationPath()
        )

        return metaAccountDao.getAliasAccount(context.value)!!.toDomain()
    }

    override suspend fun deriveWalletAccountId(entropy: ByteArray): io.paritytech.polkadotapp.common.domain.model.AccountId {
        val mnemonic = MnemonicCreator.fromEntropy(entropy)
        val secrets = accountSecretsFactory.create(
            mnemonic = mnemonic,
            encryptionType = DEFAULT_SUBSTRATE_CRYPTO_TYPE.toEncryption(),
            derivationPath = MetaAccount.Purpose.WALLET.derivationPath()
        )
        return secrets.substrateKeyPair.publicKey.substrateAccountId()
    }

    private fun BandersnatchContext.aliasAccountDerivationPath(): String = JunctionDecoder.HARD_SEPARATOR + stringValue

    private suspend fun MetaAccount.Purpose.derivationPath(): String {
        val provider = accountDerivationProviders[this]
            ?: throw IllegalStateException("No derivation provider contributed for purpose $this")

        return provider.provideDerivationPath()
    }

    private fun MetaAccount.Purpose.toLocal(): MetaAccountLocal.PurposeLocal {
        return when (this) {
            MetaAccount.Purpose.WALLET -> MetaAccountLocal.PurposeLocal.WALLET
            MetaAccount.Purpose.DEPOSIT -> MetaAccountLocal.PurposeLocal.DEPOSIT
            MetaAccount.Purpose.ALIAS -> MetaAccountLocal.PurposeLocal.ALIAS
            MetaAccount.Purpose.CANDIDATE -> MetaAccountLocal.PurposeLocal.CANDIDATE
        }
    }
}
