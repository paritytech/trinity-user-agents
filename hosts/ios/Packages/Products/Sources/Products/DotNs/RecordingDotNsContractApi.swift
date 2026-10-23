import Foundation

public final class RecordingDotNsContractApi: DotNsContractApiProtocol {
    private let contractApi: DotNsContractApiProtocol
    private let snapshot: DotNsRecordSnapshotProtocol

    public init(contractApi: DotNsContractApiProtocol, snapshot: DotNsRecordSnapshotProtocol) {
        self.contractApi = contractApi
        self.snapshot = snapshot
    }

    public func resolveContentHash(dotNsName: String) async throws -> Data {
        try await contractApi.resolveContentHash(dotNsName: dotNsName)
    }

    public func getMetadata(dotNsName: String, key: String) async throws -> String? {
        let value = try await contractApi.getMetadata(dotNsName: dotNsName, key: key)
        snapshot.store(value, name: dotNsName, key: key)
        return value
    }

    public func clearCache() {
        contractApi.clearCache()
        snapshot.clearAll()
    }
}
