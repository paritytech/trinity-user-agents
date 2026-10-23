import Foundation
import os
import SubstrateSdk

public final class DotNsReadLog: Sendable {
    private struct State {
        var records: [ProductVersion.RecordKey: DotNsSnapshotRecord] = [:]
        var contentHashes: [String: String] = [:]
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    public init() {}

    public func version(of productId: ProductId) -> ProductVersion {
        state.withLock { state in
            ProductVersion(productId: productId, records: state.records, contentHashes: state.contentHashes)
        }
    }

    func log(_ value: String?, name: String, key: String) {
        let record: DotNsSnapshotRecord = value.map { .value($0) } ?? .absent
        state.withLock { $0.records[ProductVersion.RecordKey(name: name, key: key)] = record }
    }

    func log(contentHash: Data, name: String) {
        state.withLock { $0.contentHashes[name] = contentHash.toHex() }
    }
}

public final class RecordingDotNsContractApi: DotNsContractApiProtocol {
    private let contractApi: DotNsContractApiProtocol
    private let log: DotNsReadLog

    public init(contractApi: DotNsContractApiProtocol, log: DotNsReadLog) {
        self.contractApi = contractApi
        self.log = log
    }

    public func resolveContentHash(dotNsName: String) async throws -> Data {
        let contentHash = try await contractApi.resolveContentHash(dotNsName: dotNsName)
        log.log(contentHash: contentHash, name: dotNsName)
        return contentHash
    }

    public func getMetadata(dotNsName: String, key: String) async throws -> String? {
        let value = try await contractApi.getMetadata(dotNsName: dotNsName, key: key)
        log.log(value, name: dotNsName, key: key)
        return value
    }

    public func clearCache() {
        contractApi.clearCache()
    }
}
