package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import kotlinx.coroutines.CancellationException
import uniffi.truapi.HostRejection
import uniffi.truapi.NativeWalletSecretProvider
import javax.inject.Inject

/** Reads the exact selected wallet's existing protected root record. */
class TrUAPIWalletSecretProvider @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
) : NativeWalletSecretProvider {
    override suspend fun readWalletRootEntropy(walletId: String): ByteArray = try {
        val metaId = walletId.toLongOrNull()
        require(metaId != null && metaId.toString() == walletId) { "Invalid wallet identifier" }
        requireSelectedWallet(metaId)
        val entropy = accountSecretsStorage.requireMetaAccountPassphrase(metaId).entropy
        requireSelectedWallet(metaId)
        entropy
    } catch (cancellation: CancellationException) {
        throw cancellation
    } catch (error: Exception) {
        throw HostRejection.Rejected("Selected wallet root is unavailable").apply { initCause(error) }
    }

    private suspend fun requireSelectedWallet(walletId: Long) {
        val selected = accountRepository.getWalletAccount()
        require(selected.id == walletId && selected.purpose == MetaAccount.Purpose.WALLET) {
            "Wallet selection changed during activation"
        }
    }
}
