package io.paritytech.polkadotapp.feature_transactions.api.domain.model

import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.extensions.verifySignature.GeneralTransactionSigner
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.AccountId

typealias TransactionSigner = GeneralTransactionSigner

interface SubmissionTransactionSigner : TransactionSigner

interface FeeTransactionSigner : TransactionSigner {
    /**
     * When signing for fee request we use fake keypair to not to provide real user signature and make transaction submittable on-chain
     */
    suspend fun fakeSignerId(chain: Chain): AccountId
}
