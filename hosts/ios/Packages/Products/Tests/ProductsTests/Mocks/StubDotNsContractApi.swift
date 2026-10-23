import Foundation
import os
@testable import Products

final class StubDotNsContractApi: DotNsContractApiProtocol, @unchecked Sendable {
    fileprivate struct State {
        var texts: [String: String] = [:]
        var contentHashes: [String: Data] = [:]
        var failure: Error?
        var queuedFailures: [Error] = []
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

    func failNextReads(with errors: [Error]) {
        state.withLock { $0.queuedFailures = errors }
    }

    func removeContentHash(name: String) {
        state.withLock { _ = $0.contentHashes.removeValue(forKey: name) }
    }

    func resolveContentHash(dotNsName: String) async throws -> Data {
        let (hash, failure) = state.withLock { state in
            state.callCount += 1
            return (state.contentHashes[dotNsName], state.nextFailure())
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
            return (state.texts["\(dotNsName)|\(key)"], state.nextFailure())
        }

        if let failure {
            throw failure
        }

        return text
    }

    func clearCache() {}
}

private extension StubDotNsContractApi.State {
    mutating func nextFailure() -> Error? {
        queuedFailures.isEmpty ? failure : queuedFailures.removeFirst()
    }
}
