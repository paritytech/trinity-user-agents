import Foundation

public protocol ProductMetadataRepositoryProtocol: Sendable {
    func fetchMetadata(name: String, key: String) async throws -> String?
}

public struct ProductMetadataRepository: ProductMetadataRepositoryProtocol {
    private let contractApi: DotNsContractApiProtocol

    public init(contractApi: DotNsContractApiProtocol) {
        self.contractApi = contractApi
    }

    public func fetchMetadata(name: String, key: String) async throws -> String? {
        try await contractApi.getMetadata(dotNsName: name, key: key)
    }
}
