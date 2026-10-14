package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.operation.OperationId
import io.paritytech.polkadotapp.feature_products_impl.domain.operation.ProductOperationService

class WorkerHostCalls(
    private val operationService: ProductOperationService,
    private val callingProductIdProvider: CallingProductIdProvider,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<WorkerBeginOperationParams, WorkerBeginOperationResult>("workerBeginOperation") { params ->
            callingProductIdProvider.getProductId().flatMap { productId ->
                operationService.begin(productId, params.label).map { WorkerBeginOperationResult(it.value) }
            }
        }

        bridge.registerHandler<WorkerEndOperationParams, Unit>("workerEndOperation") { params ->
            callingProductIdProvider.getProductId().flatMap { productId ->
                operationService.end(productId, OperationId(params.id))
            }
        }
    }
}

private data class WorkerBeginOperationParams(val label: String?)
private data class WorkerBeginOperationResult(val operationId: Long)
private data class WorkerEndOperationParams(val id: Long)
