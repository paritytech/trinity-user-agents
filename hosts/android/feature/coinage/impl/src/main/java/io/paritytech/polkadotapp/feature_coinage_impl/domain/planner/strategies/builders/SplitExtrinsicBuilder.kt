package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.builders

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.call
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.CoinageTransactionOrigins
import io.paritytech.polkadotapp.feature_coinage_impl.domain.model.toSplitDestinations
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import javax.inject.Inject

/** A `Coinage.split` of one coin into [outputCoins], signed by that coin's own key. */
class SplitExtrinsicBuilder @Inject constructor(
    private val coinageTransactionOrigins: CoinageTransactionOrigins,
    private val extrinsicService: ExtrinsicService,
) {
    suspend fun build(
        chain: Chain,
        coinToSplit: Coin,
        outputCoins: List<Coin>,
    ): Result<EnrichedSendableExtrinsic> = extrinsicService.buildExtrinsic(
        chain = chain,
        origin = coinageTransactionOrigins.createAsCoinOrigin(coin = coinToSplit),
        options = ExtrinsicService.SubmissionOptions(),
        formExtrinsic = {
            call(
                moduleName = "Coinage",
                callName = "split",
                arguments = autoEncodedArgs("split_into" to outputCoins.toSplitDestinations()),
            )
        },
    )
}
