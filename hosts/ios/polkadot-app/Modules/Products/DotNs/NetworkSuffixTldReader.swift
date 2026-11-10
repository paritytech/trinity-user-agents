import Foundation
import Individuality
import Products
import ChainRegistry
@preconcurrency import SubstrateStorageQuery
import StructuredConcurrency

/// Reads the TLD label from the `NetworkSuffix` pallet on the contracts chain, where the runtime
/// keeps the same suffix that personhood proof contexts are built from.
final class NetworkSuffixTldReader: Sendable {
    private let chainRegistry: ChainRegistryProtocol
    private let storageRequestFactory: StorageRequestFactoryProtocol
    private let configProvider: @Sendable () throws -> DotNsConfig

    init(
        chainRegistry: ChainRegistryProtocol,
        storageRequestFactory: StorageRequestFactoryProtocol,
        configProvider: @Sendable @escaping () throws -> DotNsConfig
    ) {
        self.chainRegistry = chainRegistry
        self.storageRequestFactory = storageRequestFactory
        self.configProvider = configProvider
    }
}

extension NetworkSuffixTldReader: DotNsTldReading {
    func readTld() async throws -> String {
        let config = try configProvider()
        let connection = try chainRegistry.getRpcConnectionOrError(for: config.contractsChainId)
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: config.contractsChainId)
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        let suffix = try await storageRequestFactory.readNetworkSuffix(
            connection: connection,
            codingFactory: codingFactory
        )

        // Product contexts are built from the stored bytes as is, so anything but a bare label is refused
        // rather than normalized into a TLD the runtime would not recognize.
        guard let tld = String(data: suffix, encoding: .utf8), Self.isBareLabel(tld) else {
            throw DotNsContractError.tldNotFound
        }

        return tld
    }
}

extension NetworkSuffixTldReader {
    private static let maxLabelLength = 63

    static func isBareLabel(_ value: String) -> Bool {
        guard let first = value.first, value.count <= maxLabelLength, isLowercaseAlphanumeric(first) else {
            return false
        }

        return value.allSatisfy { isLowercaseAlphanumeric($0) || $0 == "-" }
    }

    private static func isLowercaseAlphanumeric(_ character: Character) -> Bool {
        guard character.isASCII else { return false }

        return character.isLowercase || character.isNumber
    }
}
