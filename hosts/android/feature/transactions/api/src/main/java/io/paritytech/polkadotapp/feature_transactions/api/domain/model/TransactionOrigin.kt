package io.paritytech.polkadotapp.feature_transactions.api.domain.model

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.TransactionExtension
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.RestrictOrigins

interface TransactionOrigin {
    val signerSource: TransactionSignerSource

    val paysFees: Boolean

    suspend fun applyTo(extrinsicBuilder: ExtrinsicBuilder)
}

class SetTransactionExtensionOrigin(
    override val signerSource: TransactionSignerSource,
    private val transactionExtension: TransactionExtension,
) : TransactionOrigin {
    override val paysFees = false

    override suspend fun applyTo(extrinsicBuilder: ExtrinsicBuilder) {
        extrinsicBuilder.setTransactionExtension(transactionExtension)
        extrinsicBuilder.setTransactionExtension(RestrictOrigins(enabled = true))
    }
}

class SignedTransactionOrigin(override val signerSource: TransactionSignerSource.Signed) : TransactionOrigin {
    override val paysFees = true

    // Signature will be applied by the extrinsic builder factory since metaAccount is not null
    override suspend fun applyTo(extrinsicBuilder: ExtrinsicBuilder) {
        extrinsicBuilder.setTransactionExtension(RestrictOrigins(false))
    }
}

fun MetaAccount.asSignedOrigin(): TransactionOrigin {
    return SignedTransactionOrigin(asSignerSource())
}

fun MetaAccount.asSignerSource(): TransactionSignerSource.Signed {
    return TransactionSignerSource.FromAccount(this)
}

fun TransactionOrigin.metaAccountOrThrow(): MetaAccount {
    val source = signerSource
    require(source is TransactionSignerSource.FromAccount) {
        "'FromAccount' required, got: ${this::class.simpleName}"
    }

    return source.metaAccount
}
