import struct TrUAPIHost.ProductId
import Foundation
import Products

protocol APListRingVrfKeysHandling: Sendable {
    func listRingVrfKeys(owner: ProductId) async throws -> [RegisteredRingVrfKey]
}

final class APListRingVrfKeysHandler: @unchecked Sendable {
    private let callingProductId: ProductId?
    private let builtInKeys: BuiltInPersonhoodKeysProviding
    private let accountAccessHandler: AccountAccessPermissionHandling
    private let logger: LoggerProtocol

    init(
        callingProductId: ProductId?,
        builtInKeys: BuiltInPersonhoodKeysProviding,
        accountAccessHandler: AccountAccessPermissionHandling,
        logger: LoggerProtocol
    ) {
        self.callingProductId = callingProductId
        self.builtInKeys = builtInKeys
        self.accountAccessHandler = accountAccessHandler
        self.logger = logger
    }
}

extension APListRingVrfKeysHandler: APListRingVrfKeysHandling {
    func listRingVrfKeys(owner: ProductId) async throws -> [RegisteredRingVrfKey] {
        do {
            try await ensureAccess(to: owner)
            return try await builtInKeys.entries(owner: owner)
        } catch {
            logger.error("List ring VRF keys failed: \(error)")
            throw ListRingVrfKeysError.wrapping(error)
        }
    }
}

private extension APListRingVrfKeysHandler {
    func ensureAccess(to owner: ProductId) async throws {
        guard let callingProductId else {
            throw ListRingVrfKeysError.rejected
        }

        let granted = try await accountAccessHandler.request(
            productId: callingProductId,
            targetProductId: owner
        )

        guard granted else {
            throw ListRingVrfKeysError.rejected
        }
    }
}
