package io.paritytech.polkadotapp.feature_transfers_impl.data.type.nativeType

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.novasama.substrate_sdk_android.runtime.definitions.types.instances.AddressInstanceConstructor
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.call
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.bindAccountId
import io.paritytech.polkadotapp.chains.network.binding.bindBalance
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.balances
import io.paritytech.polkadotapp.chains.util.firstExistingCallName
import io.paritytech.polkadotapp.chains.util.instanceOf
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transfers_api.domain.model.ParsedTransferCall
import io.paritytech.polkadotapp.feature_transfers_impl.data.type.SubstrateTokenTransfersType
import javax.inject.Singleton

class NativeTransfersType @AssistedInject constructor(
    extrinsicService: ExtrinsicService,
    chainRegistry: ChainRegistry,
    @Assisted asset: Chain.Asset,
) : SubstrateTokenTransfersType(extrinsicService, chainRegistry, asset) {
    @AssistedFactory
    @Singleton
    interface Factory {
        fun create(asset: Chain.Asset): NativeTransfersType
    }

    override fun ExtrinsicBuilder.transfer(
        recipient: AccountId,
        amount: Balance,
    ) {
        nativeTransfer(accountId = recipient, amount = amount, keepAlive = false)
    }

    private fun ExtrinsicBuilder.nativeTransfer(accountId: AccountId, amount: Balance, keepAlive: Boolean = false): ExtrinsicBuilder {
        val callName = if (keepAlive) "transfer_keep_alive" else runtime.metadata.balances().firstExistingCallName("transfer_allow_death", "transfer")

        return call(
            moduleName = Modules.BALANCES,
            callName = callName,
            // Not using autoEncodedArgs here since "dest" might encode both to MultiAddress and AccountId on different chains
            // AddressInstanceConstructor will take care of that
            arguments = mapOf(
                "dest" to AddressInstanceConstructor.constructInstance(runtime.typeRegistry, accountId.value),
                "value" to amount.value
            )
        )
    }

    override suspend fun parseTransferCall(call: GenericCall.Instance): ParsedTransferCall? {
        val isOutCall = call.instanceOf(Modules.BALANCES, "transfer_keep_alive", "transfer_allow_death", "transfer")
        if (!isOutCall) return null

        return runCatching {
            ParsedTransferCall(
                recipient = bindAccountId(call.arguments["dest"]),
                amount = bindBalance(call.arguments["value"])
            )
        }
            .logFailure("Failed to parse transfer call")
            .getOrNull()
    }
}
