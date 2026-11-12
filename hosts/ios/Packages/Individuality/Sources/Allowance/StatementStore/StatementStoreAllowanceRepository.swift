import ChainStore
import Foundation
import SubstrateSdk
import SubstrateStorageQuery

public protocol StatementStoreAllowanceRepositoryProtocol: Sendable {
    /// Whether each account holds a statement store allowance at `blockHash`. An account absent from the
    /// result was not answered.
    func hasAllowance(_ accounts: [AccountId], at blockHash: Data) async throws -> [AccountId: Bool]
}

/// Reads the runtime's well-known `:statement_allowance:` keys, whichever pallet granted the allowance.
public final class StatementStoreAllowanceRepository: @unchecked Sendable {
    private static let allowanceKeyPrefix = Data(":statement_allowance:".utf8)

    private let chainId: ChainId
    private let chainRegistry: ChainResourceProtocol
    private let storageRequestFactory: StorageRequestFactoryProtocol

    public init(
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        storageRequestFactory: StorageRequestFactoryProtocol
    ) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.storageRequestFactory = storageRequestFactory
    }
}

extension StatementStoreAllowanceRepository: StatementStoreAllowanceRepositoryProtocol {
    public func hasAllowance(_ accounts: [AccountId], at blockHash: Data) async throws -> [AccountId: Bool] {
        guard !accounts.isEmpty else { return [:] }

        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
        let keyByAccount = Dictionary(
            accounts.map { ($0, Self.allowanceKeyPrefix + $0) },
            uniquingKeysWith: { first, _ in first }
        )

        let updates = try await storageRequestFactory.queryRawItems(
            for: { Array(keyByAccount.values) },
            at: blockHash,
            engine: connection
        )
        .asyncExecute()

        let answered = updates
            .flatMap { $0 }
            .flatMap { StorageUpdateData(update: $0).changes }
            .reduce(into: [Data: Bool]()) { result, change in
                result[change.key] = change.value != nil
            }

        return keyByAccount.reduce(into: [:]) { result, pair in
            result[pair.key] = answered[pair.value]
        }
    }
}
