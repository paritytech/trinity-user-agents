package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import com.google.gson.annotations.JsonAdapter
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AllocationOutcome
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.DerivationIndexWire
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.DerivationIndexWireAdapter
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.serialization.toDomain
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge

class AllowanceHostCalls(
    private val botApi: ProductsBotApi,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<RequestResourceAllocationParams, RequestResourceAllocationResult>(
            "hostRequestResourceAllocation"
        ) { params ->
            val productId = callingProductIdProvider.getProductId().getOrThrow()
            val resources = params.resources.map { it.toDomain() }
            botApi.requestResourceAllocation(productId, resources).map { outcomes ->
                RequestResourceAllocationResult(outcomes = outcomes.map { it.toDto() })
            }
        }
    }
}

private data class RequestResourceAllocationParams(val resources: List<AllocatableResourceDto>)

private data class RequestResourceAllocationResult(val outcomes: List<AllocationOutcomeDto>)

private data class AllocatableResourceDto(
    val kind: String,
    @JsonAdapter(DerivationIndexWireAdapter::class)
    val dest: DerivationIndexWire? = null,
)

private data class AllocationOutcomeDto(val kind: String)

private fun AllocatableResourceDto.toDomain(): AllocatableResource = when (kind) {
    "BulletinAllowance" -> AllocatableResource.BulletInAllowance
    "StatementStoreAllowance" -> AllocatableResource.StatementStoreAllowance
    "SmartContractAllowance" -> AllocatableResource.SmartContractAllowance(
        dest = requireNotNull(dest) { "SmartContractAllowance requires `dest`" }.toDomain().getOrThrow()
    )
    "AutoSigning" -> AllocatableResource.AutoSigning
    else -> throw IllegalArgumentException("Unknown AllocatableResource kind: $kind")
}

private fun AllocationOutcome.toDto(): AllocationOutcomeDto = AllocationOutcomeDto(
    kind = when (this) {
        AllocationOutcome.Allocated -> "Allocated"
        AllocationOutcome.Rejected -> "Rejected"
        AllocationOutcome.NotAvailable -> "NotAvailable"
    }
)
