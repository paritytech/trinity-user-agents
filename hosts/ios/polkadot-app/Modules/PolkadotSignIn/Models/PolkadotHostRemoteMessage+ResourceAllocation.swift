import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk
import Individuality

extension PolkadotHostRemoteMessage {
    struct ResourceAllocationRequest {
        let resources: [AllocatableResource]
        let onExisting: OnExistingAllowancePolicy
    }
}

// MARK: - SCALE Coding

extension PolkadotHostRemoteMessage.ResourceAllocationRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        resources = try [AllocatableResource](scaleDecoder: scaleDecoder)
        onExisting = try OnExistingAllowancePolicy(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try resources.encode(scaleEncoder: scaleEncoder)
        try onExisting.encode(scaleEncoder: scaleEncoder)
    }
}
