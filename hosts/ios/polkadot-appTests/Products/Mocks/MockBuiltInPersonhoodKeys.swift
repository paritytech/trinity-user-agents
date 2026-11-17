import struct TrUAPIHost.ProductId
import Foundation
import KeyDerivation
import Products

@testable import polkadot_app

final class MockBuiltInPersonhoodKeys: BuiltInPersonhoodKeysProviding, @unchecked Sendable {
    var entries: [RegisteredRingVrfKey] = []
    var entriesError: Error?
    var resolvedKeyManager: BandersnatchKeyManaging = MockBandersnatchKeyManager()
    var resolveError: Error?
    private(set) var recordedOwners: [ProductId] = []
    private(set) var recordedHandles: [RingVrfKeyHandle] = []

    func entries(owner: ProductId) async throws -> [RegisteredRingVrfKey] {
        recordedOwners.append(owner)

        if let entriesError {
            throw entriesError
        }

        return entries
    }

    func resolve(handle: RingVrfKeyHandle, ring _: RingLocation) throws -> BandersnatchKeyManaging {
        recordedHandles.append(handle)

        if let resolveError {
            throw resolveError
        }

        return resolvedKeyManager
    }
}
