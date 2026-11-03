import AsyncExtensions
import Foundation
import os
import StatementDelivery
import StatementStore
import Testing

struct OutgoingStatementServiceTests {
    private let input = TestInput(statementId: "A")
    private let connection = InMemoryStatementConnection()

    @Test func copyFromThePeriodsOneShotAccountIsStored() async throws {
        try await submitCopy(signedBy: "one-shot/A/10")

        #expect(try await makeService().isStored(input, period: 10))
    }

    @Test func copyFromTheIdentityAccountIsStored() async throws {
        try await submitCopy(signedBy: "identity")

        #expect(try await makeService().isStored(input, period: 10))
    }

    @Test func copyFromAnotherPeriodIsNotTheCurrentCopy() async throws {
        try await submitCopy(signedBy: "one-shot/A/9")

        #expect(try await !makeService().isStored(input, period: 10))
    }

    @Test func publishSignsWithTheGivenSigner() async throws {
        try await makeService().publish(input, signer: StubSigner(keyId: "one-shot/A/10"))

        #expect(try await makeService().isStored(input, period: 10))
    }
}

private extension OutgoingStatementServiceTests {
    func makeService() -> OutgoingStatementService<TestInput> {
        OutgoingStatementService(builder: FakeBuilder(), connection: connection, signerManager: StubSignerManager())
    }

    func submitCopy(signedBy keyId: String) async throws {
        try await connection.submitStatement(
            with: FakeBuilder().makeUnsignedStatement(for: input).signed(by: StubSigner(keyId: keyId))
        )
    }
}

/// Returns every submitted statement for any filter; enough for single-topic tests.
final class InMemoryStatementConnection: StatementStoreConnecting, @unchecked Sendable {
    private let statements = OSAllocatedUnfairLock<[Data]>(initialState: [])

    func submitStatement(with builder: StatementSubmitParametersBuilding) async throws {
        let encoded = try builder.build().encodedStatement
        statements.withLock { $0.append(encoded) }
    }

    func fetchStatements(with _: TopicFilter) async throws -> [Data] {
        statements.withLock { $0 }
    }

    func subscribeStatements(with _: TopicFilter) throws -> AnyAsyncSequence<StatementsPage> {
        AsyncStream<StatementsPage> { $0.finish() }.eraseToAnyAsyncSequence()
    }
}
