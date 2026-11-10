import Foundation
import StatementStore
import SubstrateSdk

public protocol OutgoingStatementServicing<Input>: Sendable {
    associatedtype Input: OutgoingStatementInput

    func publish(_ input: Input, signer: StatementStoreSigning) async throws

    /// Whether a copy published for `period` is still in the store.
    func isStored(_ input: Input, period: UInt32) async throws -> Bool
}

public final class OutgoingStatementService<Input: OutgoingStatementInput>: @unchecked Sendable {
    private let builder: any OutgoingStatementBuilding<Input>
    private let connection: StatementStoreConnecting
    private let signerManager: StatementStoreSignerManaging
    private let submitter: SigningStatementSubmitter

    public init(
        builder: any OutgoingStatementBuilding<Input>,
        connection: StatementStoreConnecting,
        signerManager: StatementStoreSignerManaging
    ) {
        self.builder = builder
        self.connection = connection
        self.signerManager = signerManager
        submitter = SigningStatementSubmitter(connection: connection)
    }
}

extension OutgoingStatementService: OutgoingStatementServicing {
    public func publish(_ input: Input, signer: StatementStoreSigning) async throws {
        try await submitter.submit(builder.makeUnsignedStatement(for: input), signer: signer)
    }

    // A copy is signed either by the period's one-shot account or, after consent, by the identity account.
    public func isStored(_ input: Input, period: UInt32) async throws -> Bool {
        let copySigners: Set<AccountId> = try [
            signerManager.makeSigner(for: input.oneShotKeyId(period: period)).accountId,
            signerManager.makeSigner(for: input.identitySignKeyId).accountId
        ]

        let statements = try await connection.fetchStatements(with: builder.storageTopicFilter(for: input))

        // Anyone can post under the topic, so a statement we can't parse is skipped rather than failing the check.
        return statements.contains { data in
            guard let statement = try? Statement(scaleDecoder: ScaleDecoder(data: data)) else {
                return false
            }

            return statement.getSenderAccountId().map(copySigners.contains) ?? false
        }
    }
}
