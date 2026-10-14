import Foundation
import SubstrateSdk

public protocol StatementStoreSignerManaging {
    func makeSigner(for signerKeyId: String) throws -> StatementStoreSigning
}

public final class ClosureSignerManager: StatementStoreSignerManaging {
    private let closure: (String) throws -> StatementStoreSigning

    public init(closure: @escaping (String) throws -> StatementStoreSigning) {
        self.closure = closure
    }

    public func makeSigner(for signerKeyId: String) throws -> any StatementStoreSigning {
        try closure(signerKeyId)
    }
}

public protocol StatementStoreSigning {
    var accountId: AccountId { get }

    func sign(_ data: Data) throws -> StatementProof
}
