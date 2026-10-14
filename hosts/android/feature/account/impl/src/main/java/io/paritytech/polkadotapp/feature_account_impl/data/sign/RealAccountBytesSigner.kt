package io.paritytech.polkadotapp.feature_account_impl.data.sign

import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper
import io.novasama.substrate_sdk_android.encrypt.Signer
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.MultiSignature
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.util.signMultiSignature
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_account_api.data.sign.AccountBytesSigner
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.BandersnatchSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getMetaAccountKeypairOrThrow
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.getMetaAccountSr25519Keypair
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.sign
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.determineEncryptionOrThrow
import kotlinx.coroutines.withContext
import javax.inject.Inject

class RealAccountBytesSigner @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
    private val bandersnatchSecretsStorage: BandersnatchSecretsStorage,
    private val coroutineDispatchers: CoroutineDispatchers,
) : AccountBytesSigner {
    override suspend fun signRawBytesByWallet(
        message: ByteArray,
        chainId: ChainId,
        context: MessageSigningContext,
    ): Result<SignatureWrapper> = withContext(coroutineDispatchers.io) {
        runCatching {
            val metaAccount = accountRepository.getWalletAccount()
            val keypair = accountSecretsStorage.getMetaAccountKeypairOrThrow(metaAccount.id)
            val accountId = metaAccount.accountIdIn(chainRegistry.getChain(chainId))
            val multiChainEncryption = metaAccount.determineEncryptionOrThrow(accountId)

            Signer.sign(multiChainEncryption, context.messageInContext(message), keypair, skipHashing = true)
        }
    }

    override suspend fun signRawBytesByCandidate(message: ByteArray, context: MessageSigningContext): MultiSignature {
        val candidateAccount = accountRepository.getCandidateAccount()
        return signRawBytes(message, context, candidateAccount)
    }

    override suspend fun signRawBytes(
        message: ByteArray,
        context: MessageSigningContext,
        account: MetaAccount,
    ): MultiSignature {
        return withContext(coroutineDispatchers.io) {
            val keypair = accountSecretsStorage.getMetaAccountSr25519Keypair(account.id)
            keypair.signMultiSignature(message, context)
        }
    }

    override suspend fun signWithBandersnatchByWallet(message: ByteArray, context: MessageSigningContext) =
        withContext(coroutineDispatchers.io) {
            runCatching {
                val metaId = accountRepository.getWalletAccount().id
                bandersnatchSecretsStorage.sign(metaId, context.messageInContext(message))
            }
        }
}
