package io.paritytech.polkadotapp.feature_transactions.api.data

import io.novasama.substrate_sdk_android.encrypt.MultiChainEncryption
import io.novasama.substrate_sdk_android.encrypt.keypair.Keypair
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.FeeTransactionSigner
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SubmissionTransactionSigner
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import okio.`-DeprecatedOkio`.source

interface SignerProvider {
    suspend fun submissionSigner(metaAccount: MetaAccount): SubmissionTransactionSigner

    suspend fun submissionSigner(keypair: Keypair, encryption: MultiChainEncryption): SubmissionTransactionSigner

    suspend fun feeSigner(metaAccount: MetaAccount): FeeTransactionSigner

    suspend fun feeSigner(keypair: Keypair, encryption: MultiChainEncryption): FeeTransactionSigner
}

suspend fun SignerProvider.submissionSigner(source: TransactionSignerSource): SubmissionTransactionSigner? {
    return when (source) {
        is TransactionSignerSource.FromAccount -> submissionSigner(source.metaAccount)
        is TransactionSignerSource.FromKeyPair -> submissionSigner(source.keypair, source.encryption)
        TransactionSignerSource.None -> null
    }
}

suspend fun SignerProvider.feeSigner(source: TransactionSignerSource): FeeTransactionSigner? {
    return when (source) {
        is TransactionSignerSource.FromAccount -> feeSigner(source.metaAccount)
        is TransactionSignerSource.FromKeyPair -> feeSigner(source.keypair, source.encryption)
        TransactionSignerSource.None -> null
    }
}

suspend fun SignerProvider.submissionSignerOrThrow(source: TransactionSignerSource): SubmissionTransactionSigner {
    return requireNotNull(submissionSigner(source)) {
        "Submission signer was not found for origin ${source::class.simpleName}"
    }
}

suspend fun SignerProvider.feeSignerOrThrow(source: TransactionSignerSource): FeeTransactionSigner {
    return requireNotNull(feeSigner(source)) {
        "Fee signer was not found for origin ${source::class.simpleName}"
    }
}
