import Testing
import Foundation
import SubstrateSdk
@testable import HandoffService

struct HandoffServiceAckTests {
    @Test func ackTreatsNotFoundAsSuccess() async throws {
        let engine = MockJSONRPCEngine()
        engine.result = .failure(JSONRPCError(message: "not found", code: HOPErrorCode.notFound, data: nil))
        let service = HandoffService(connection: engine)

        // Base spec: the entry may already be removed (acked elsewhere or promoted) — benign.
        try await service.acknowledgeReceivedData(
            by: Data(1 ... 32),
            recipient: MockRecipientProofProvider()
        )

        #expect(engine.lastMethod == "hop_ack")
    }

    @Test func ackRethrowsOtherErrors() async throws {
        let engine = MockJSONRPCEngine()
        engine.result = .failure(JSONRPCError(message: "internal", code: 1_000, data: nil))
        let service = HandoffService(connection: engine)

        await #expect(throws: JSONRPCError.self) {
            try await service.acknowledgeReceivedData(
                by: Data(1 ... 32),
                recipient: MockRecipientProofProvider()
            )
        }
    }
}
