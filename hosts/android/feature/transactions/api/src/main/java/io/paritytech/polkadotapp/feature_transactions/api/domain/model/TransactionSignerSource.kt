package io.paritytech.polkadotapp.feature_transactions.api.domain.model

import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.CheckNonce
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.verifySignature.VerifySignature
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.accountIdOf
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount

/**
 * Source for the [TransactionSigner] that prepare signature for [VerifySignature] and account id to get nonce for [CheckNonce]
 */
sealed interface TransactionSignerSource {
    /**
     * There is no signer associated with the origin
     */
    object None : TransactionSignerSource

    /**
     * Intermediate type to indicate subset of variants that result in valid signed origin (not None)
     */
    sealed interface Signed : TransactionSignerSource

    /**
     * Signer should be inferred from the local account, represented by [metaAccount]
     */
    class FromAccount(val metaAccount: MetaAccount) : Signed

    /**
     * Signing should be done with an arbitrary [keypair]
     */
    class FromKeyPair(val keypair: Keypair, val encryption: MultiChainEncryption) : Signed
}

fun TransactionSignerSource.accountId(chain: Chain): AccountId? {
    return when (this) {
        is TransactionSignerSource.Signed -> accountId(chain)
        TransactionSignerSource.None -> null
    }
}

fun TransactionSignerSource.Signed.accountId(chain: Chain): AccountId {
    return when (this) {
        is TransactionSignerSource.FromAccount -> metaAccount.accountIdIn(chain)
        is TransactionSignerSource.FromKeyPair -> chain.accountIdOf(keypair.publicKey)
    }
}

fun TransactionSignerSource.accountIdOrThrow(chain: Chain): AccountId {
    return requireNotNull(accountId(chain)) {
        "Account id for ${this::class.simpleName} is not defined"
    }
}
