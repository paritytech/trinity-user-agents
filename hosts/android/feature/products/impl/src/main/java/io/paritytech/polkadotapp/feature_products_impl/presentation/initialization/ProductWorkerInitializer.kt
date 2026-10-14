package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_products_impl.domain.operation.ProductOperationService
import kotlinx.coroutines.launch
import javax.inject.Inject

// Resumes funding operations left open by a previous run, re-acquiring the worker for each so the
// flow can continue where it left off.
class ProductWorkerInitializer @Inject constructor(
    private val operationService: ProductOperationService,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        scope.launch {
            runCancellableCatching { operationService.resumeOpenOperations() }
                .logFailure("Failed to resume open funding operations")
        }
        return Result.success(Unit)
    }
}
