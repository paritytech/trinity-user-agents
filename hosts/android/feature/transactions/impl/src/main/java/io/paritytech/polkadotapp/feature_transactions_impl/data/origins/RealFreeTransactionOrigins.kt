package io.paritytech.polkadotapp.feature_transactions_impl.data.origins

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.call
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.FreeTransactionOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.PeopleLiteAuth
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.RestrictOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import javax.inject.Inject

class RealFreeTransactionOrigins @Inject constructor(
    private val knownChains: KnownChains,
    private val accountRepository: AccountRepository,
    private val signedOrigins: SignedOrigins,
) : FreeTransactionOrigins {
    override suspend fun freeTxFromWalletOrSigned(chainId: ChainId): TransactionOrigin {
        return when (chainId) {
            knownChains.people -> UseLitePersonFreeTxOrigin(accountRepository.getWalletAccount())
            else -> signedOrigins.wallet()
        }
    }
}

private class UseLitePersonFreeTxOrigin(metaAccount: MetaAccount) : TransactionOrigin {
    override val paysFees: Boolean = false

    override val signerSource = TransactionSignerSource.FromAccount(metaAccount)

    override suspend fun applyTo(extrinsicBuilder: ExtrinsicBuilder) {
        val call = extrinsicBuilder.getWrappedCall()

        extrinsicBuilder.resetCalls()

        extrinsicBuilder
            .setTransactionExtension(RestrictOrigins(true))
            .setTransactionExtension(PeopleLiteAuth())
            .call(
                moduleName = Modules.PEOPLE_LITE,
                callName = "dispatch_as_signer",
                arguments = mapOf("call" to call)
            )
    }
}
