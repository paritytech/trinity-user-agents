import Foundation

public final class StatementPermissiveProofVerifier: StatementStoreProofVerifying {
    public init() {}

    public func verifyProof(for _: Statement) -> Bool {
        true
    }
}
