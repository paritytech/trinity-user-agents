package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import javax.inject.Inject

/** Secret material and display identity for the core's wallet-local session. */
class TrUAPILocalSession(
    val secret: ByteArray,
    val liteUsername: String?,
)

/**
 * Resolves the wallet-local signing session the Rust core runs on.
 *
 * Until SSO pairing moves into the core (truapi#334) the core has no session of
 * its own, and without one it can hold no keys — so every account and signing
 * call fails regardless of what the user approves. iOS activates the same local
 * session, which is why signing works there.
 *
 * The secret is raw BIP-39 entropy because the core derives the session's root
 * and identity keypairs from it directly; handing it anything derived would give
 * the same recovery phrase different product accounts than iOS derives.
 *
 * The username is read locally, as the iOS host reads it from its settings: the
 * claim and recovery flows write it there, and awaiting a chain read here would
 * hold the runtime boot on a connection that may not be up yet.
 */
class TrUAPILocalSessionSource @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
    private val localUsernameStorage: LocalUsernameStorage,
) {
    suspend fun resolve(): Result<TrUAPILocalSession> = runCatching {
        val account = accountRepository.getWalletAccount()

        TrUAPILocalSession(
            secret = accountSecretsStorage.requireMetaAccountPassphrase(account.id).entropy,
            // Display metadata only, so a missing username still yields a session.
            liteUsername = localUsernameStorage.getValue()?.getDisplayUsername(),
        )
    }
}
