package io.paritytech.polkadotapp.feature_fund_impl.domain.fund.model

import io.paritytech.polkadotapp.chains.multiNetwork.ChainWithAsset
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount

class FundCredentials(
    val depositAccount: MetaAccount,
    val chainWithAsset: ChainWithAsset,
)

fun FundCredentials.depositAddress(): String {
    return depositAccount.addressIn(chainWithAsset.chain)
}
