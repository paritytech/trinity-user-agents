package io.paritytech.polkadotapp.feature_transfers_impl.data.type.orml

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.definitions.types.instances.AddressInstanceConstructor
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.call
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.currencyId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.currencyIdHex
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.bindAccountId
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.asRawScaleValue
import io.paritytech.polkadotapp.chains.util.instanceOf
import io.paritytech.polkadotapp.chains.util.requireOrml
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.ParsedTransferCall
import io.paritytech.polkadotapp.feature_transfers_impl.data.type.SubstrateTokenTransfersType
import javax.inject.Singleton

class OrmlTokenTransfersType @AssistedInject constructor(
    extrinsicService: ExtrinsicService,
    private val chainRegistry: ChainRegistry,
    @Assisted asset: Chain.Asset,
) : SubstrateTokenTransfersType(extrinsicService, chainRegistry, asset) {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(asset: Chain.Asset): OrmlTokenTransfersType
    }

    private val ormlType = asset.requireOrml()

    override fun ExtrinsicBuilder.transfer(recipient: AccountId, amount: Balance) {
        call(
            moduleName = Modules.TOKENS,
            callName = "transfer",
            // Not using autoEncodedArgs here since "dest" might encode both to MultiAddress and AccountId on different chains
            // AddressInstanceConstructor will take care of that
            arguments = mapOf(
                "dest" to AddressInstanceConstructor.constructInstance(
                    runtime.typeRegistry,
                    recipient.value
                ),
                "currency_id" to ormlType.currencyId(runtime).value,
                "amount" to amount.value
            )
        )
    }

    override suspend fun parseTransferCall(call: GenericCall.Instance): ParsedTransferCall? {
        if (!call.instanceOf(Modules.TOKENS, "transfer")) return null

        return runCatching {
            val runtime = chainRegistry.getRuntime(chainAsset.chainId)

            val currencyIdRaw = call.arguments["currency_id"].asRawScaleValue()
            val currencyId = ormlType.currencyIdHex(runtime, currencyIdRaw)
            if (currencyId != ormlType.currencyIdScale) return null

            ParsedTransferCall(
                recipient = bindAccountId(call.arguments["dest"]),
                amount = bindBalance(call.arguments["amount"])
            )
        }
            .logFailure("Failed to parse transfer call")
            .getOrNull()
    }
}
