import Foundation
import ExtrinsicService
import NovaCrypto
import SubstrateSdk
import BulletinChain
import StructuredConcurrency

// MARK: - Preimage

extension ProductsNativeApi {
    func lookupPreimage(hash: Data) async throws -> Data {
        try await markStallActivity("Lookup preimage") {
            try await markStallRegion("Lookup") {
                let ipfsFetcher = IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main)
                return try await ipfsFetcher.lookupBy(rawHash: hash)
            }
        }
    }

    func submitPreimage(data: Data) async throws -> String {
        try await markStallActivity("Submit preimage") {
            try await markStallRegion("Submit") {
                let wallet = try await preimageSponsor.sponsor(productId: productId, data: data)

                let chain = try chainRegistry.getChainOrError(for: AppConfig.Chains.bulletInChain)

                let bulletinFacade = ExtrinsicSubmissionMonitorFacade(
                    chainRegistry: chainRegistry,
                    substrateStorageFacade: substrateStorageFacade,
                    operationQueue: operationQueue,
                    extrinsicVersion: .V4
                )

                let monitorFactory = try bulletinFacade.createMonitorFactory(chain: chain)

                let originFactory = SignedExtrinsicOriginFactory(
                    chainRegistry: chainRegistry,
                    operationQueue: operationQueue,
                    logger: logger
                )

                let origin = try originFactory.extrinsicOriginDefiner(
                    from: wallet,
                    chain: chain
                )

                let storeCall = TransactionStoragePallet.StoreCall(data: data)

                try await monitorFactory.submitAndMonitorWrapper(
                    extrinsicBuilderClosure: { builder in
                        try builder.adding(call: storeCall.runtimeCall())
                    },
                    origin: origin,
                    params: ExtrinsicSubmissionParams(feeAssetId: nil, eventsMatcher: nil)
                )
                .asyncExecute()
                .ensureSuccess()

                return try data.blake2b32().toHex(includePrefix: true)
            }
        }
    }
}
