import Foundation
import os
@testable import Products

final class StubDotNsContractApi: DotNsContractApiProtocol, @unchecked Sendable {
    private struct State {
        var texts: [String: String] = [:]
        var contentHashes: [String: Data] = [:]
        var failure: Error?
        var callCount = 0
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var callCount: Int { state.withLock { $0.callCount } }

    func set(text: String, name: String, key: String) {
        state.withLock { $0.texts["\(name)|\(key)"] = text }
    }

    func set(contentHash: Data, name: String) {
        state.withLock { $0.contentHashes[name] = contentHash }
    }

    func failAllReads(with error: Error) {
        state.withLock { $0.failure = error }
    }

    func resolveContentHash(dotNsName: String) async throws -> Data {
        let (hash, failure) = state.withLock { state in
            state.callCount += 1
            return (state.contentHashes[dotNsName], state.failure)
        }

        if let failure {
            throw failure
        }

        guard let hash else {
            throw DotNsContractError.contentHashNotFound
        }

        return hash
    }

    func getMetadata(dotNsName: String, key: String) async throws -> String? {
        let (text, failure) = state.withLock { state in
            state.callCount += 1
            return (state.texts["\(dotNsName)|\(key)"], state.failure)
        }

        if let failure {
            throw failure
        }

        return text
    }

    func clearCache() {}
}
