import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk
import Individuality

extension PolkadotHostRemoteMessage {
    struct ResourceAllocationRequest {
        let callingProduct: ProductId
        let resources: [AllocatableResource]
        let onExisting: OnExistingAllowancePolicy
    }
}

// MARK: - SCALE Coding

extension PolkadotHostRemoteMessage.ResourceAllocationRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        callingProduct = try String(scaleDecoder: scaleDecoder)
        resources = try [AllocatableResource](scaleDecoder: scaleDecoder)
        onExisting = try OnExistingAllowancePolicy(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try callingProduct.encode(scaleEncoder: scaleEncoder)
        try resources.encode(scaleEncoder: scaleEncoder)
        try onExisting.encode(scaleEncoder: scaleEncoder)
    }
}
