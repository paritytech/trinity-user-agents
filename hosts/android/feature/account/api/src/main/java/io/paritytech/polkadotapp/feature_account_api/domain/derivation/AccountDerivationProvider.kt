package io.paritytech.polkadotapp.feature_account_api.domain.derivation

/**
 * Supplies the sr25519 derivation path of a built-in account.
 *
 * Contributed per [io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount.Purpose] by the
 * module that owns the scheme, so `account` never has to know which subtree a feature derives from.
 */
interface AccountDerivationProvider {
    suspend fun provideDerivationPath(): String
}
