import Foundation
import Individuality
import SubstrateOperation

/// The single shared cache for Resources pallet parameters.
///
/// Providers still take `ResourcesParametersProviding` through `init`; this is only the default
/// used at the app's composition roots, so a burst of allowance operations shares one cache.
enum ResourcesParametersFacade {
    static let shared: ResourcesParametersProviding = CachedResourcesParametersProvider(
        viewFunctionExecutor: ViewFunctionExecutor(
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            operationQueue: OperationManagerFacade.sharedDefaultQueue
        )
    )
}
