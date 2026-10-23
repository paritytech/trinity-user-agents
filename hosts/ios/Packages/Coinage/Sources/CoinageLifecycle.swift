import CryptoKit
import Foundation
import KeyDerivation

public enum CoinageLifecycleError: Error, Equatable {
    case inactive
    case staleOperation
    case rootChanged
    case readOnlyEntropy
}

/// Binds one native Coinage graph to its original wallet and activation. Only a root fingerprint is
/// retained: every derivation and registration still reads Keychain and fails while it is locked.
public final class CoinageLifecycle: RootEntropyManaging, @unchecked Sendable {
    public struct Operation: Sendable {
        fileprivate let owner: UUID
        fileprivate let generation: UUID
    }

    /// Inherited by child Tasks, including SerialOperationQueue's unstructured operation Tasks.
    @TaskLocal public static var operation: Operation?

    private let rootEntropyManager: any RootEntropyManaging
    private let rootFingerprint: SHA256.Digest
    private let owner = UUID()
    private let lock = NSLock()
    private var generation: UUID?

    public init(rootEntropyManager: any RootEntropyManaging) throws {
        self.rootEntropyManager = rootEntropyManager
        rootFingerprint = SHA256.hash(data: try rootEntropyManager.fetchRootEntropy())
    }

    /// Persistent incoming-payment ownership, independent of the transient activation ticket.
    /// The versioned encoding separates wallets, chains and pallet instances without ambiguous joins.
    public func ownerId(chainId: String, instanceId: CoinageInstanceId) -> Data {
        var identity = Data("native-coinage-incoming-owner-v1".utf8)
        identity.append(contentsOf: rootFingerprint)
        var chainLength = UInt64(chainId.utf8.count).littleEndian
        withUnsafeBytes(of: &chainLength) { identity.append(contentsOf: $0) }
        identity.append(contentsOf: chainId.utf8)
        var instance = instanceId.littleEndian
        withUnsafeBytes(of: &instance) { identity.append(contentsOf: $0) }
        return Data(SHA256.hash(data: identity))
    }

    /// Deactivation is synchronous. Reactivation never makes an old operation's ticket valid again.
    /// A replaced or unreadable root cannot activate this service; the coordinator must recreate it.
    @discardableResult
    public func setActive(_ active: Bool) -> Bool {
        lock.withLock {
            guard active else {
                generation = nil
                return true
            }
            do {
                _ = try fetchBoundRoot()
                if generation == nil { generation = UUID() }
                return true
            } catch {
                generation = nil
                return false
            }
        }
    }

    public func captureOperation() throws -> Operation {
        try lock.withLock {
            try Task.checkCancellation()
            guard let generation else { throw CoinageLifecycleError.inactive }
            let ticket = Self.operation ?? Operation(owner: owner, generation: generation)
            try validate(ticket)
            _ = try fetchBoundRoot()
            return ticket
        }
    }

    public func check(_ ticket: Operation) throws {
        try withEffect(ticket) {}
    }

    public func checkCurrentOperation() throws {
        _ = try captureOperation()
    }

    public func withOperation<T>(_ body: () async throws -> T) async throws -> T {
        let ticket = try captureOperation()
        return try await Self.$operation.withValue(ticket, operation: body)
    }

    /// Runs the final synchronous registration hook while deactivation is excluded. This is called
    /// inside the durability engine's write transaction, not merely before its signing/network awaits.
    func withEffect<T>(_ ticket: Operation, _ body: () throws -> T) throws -> T {
        try lock.withLock {
            try Task.checkCancellation()
            try validate(ticket)
            _ = try fetchBoundRoot()
            return try body()
        }
    }

    public func fetchRootEntropy() throws -> Data {
        try lock.withLock {
            try Task.checkCancellation()
            guard let generation else { throw CoinageLifecycleError.inactive }
            try validate(Self.operation ?? Operation(owner: owner, generation: generation))
            return try fetchBoundRoot()
        }
    }

    public func hasRootEntropy() throws -> Bool {
        _ = try fetchRootEntropy()
        return true
    }

    public func createRootEntropy(_: Data) throws {
        throw CoinageLifecycleError.readOnlyEntropy
    }

    private func validate(_ ticket: Operation) throws {
        guard let generation else { throw CoinageLifecycleError.inactive }
        guard ticket.owner == owner, ticket.generation == generation else {
            throw CoinageLifecycleError.staleOperation
        }
    }

    private func fetchBoundRoot() throws -> Data {
        let entropy = try rootEntropyManager.fetchRootEntropy()
        guard SHA256.hash(data: entropy) == rootFingerprint else {
            throw CoinageLifecycleError.rootChanged
        }
        return entropy
    }
}
