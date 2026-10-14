import ExtrinsicService
import ExtrinsicServiceExt
import Foundation
import SubstrateSdk

/// The chain-bound tools the engine needs to build and broadcast an extrinsic, resolved per chain.
///
/// Injected by the app, which owns chain configuration, fork protection and signing wiring; the engine
/// only asks for them by `chainId`, as its domains declare it.
public protocol DurableChainToolsProviding: Sendable {
    func extrinsicOperationFactory(for chainId: ChainId) async throws -> any ExtrinsicOperationFactoryProtocol

    /// A submitter that follows an extrinsic until its block is finalized — durability must observe the
    /// finalized outcome, not just inclusion.
    func extrinsicSubmitter(for chainId: ChainId) async throws -> any ExtrinsicSubmitting
}
