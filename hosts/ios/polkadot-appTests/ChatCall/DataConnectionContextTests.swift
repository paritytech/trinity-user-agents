@testable import polkadot_app
import Foundation
import Testing

enum DataConnectionContextTests {
    @Test("Flush sends buffered signals without batching")
    static func flushSendsBufferedSignalsWithoutBatching() async throws {
        let signaler = MockPeerConnectionSignaler()
        let sut = DataConnectionContext(
            signaler: signaler,
            logger: MockLogger()
        )
        let candidates = [
            SdpCoderTests.makeIPv4Candidate(foundation: "1", ip: "192.168.1.1", port: 1_001),
            SdpCoderTests.makeIPv4Candidate(foundation: "2", ip: "192.168.1.2", port: 1_002),
            SdpCoderTests.makeIPv4Candidate(foundation: "3", ip: "192.168.1.3", port: 1_003)
        ]

        for candidate in candidates {
            await sut.append(.candidates([candidate]))
        }

        await sut.performFlushBuffer(with: 3)

        let sentSignals = signaler.sentSignals
        let expectedSignals = candidates.map { PeerConnectionSignal.candidates([$0]) }
        #expect(sentSignals == expectedSignals)
        #expect(signaler.sentBatches == [expectedSignals])
    }

    @Test("Flush respects limit without changing signal shape")
    static func flushRespectsLimitWithoutChangingSignalShape() async throws {
        let signaler = MockPeerConnectionSignaler()
        let sut = DataConnectionContext(
            signaler: signaler,
            logger: MockLogger()
        )
        let firstCandidate = SdpCoderTests.makeIPv4Candidate(
            foundation: "1",
            ip: "192.168.1.1",
            port: 1_001
        )
        let secondCandidate = SdpCoderTests.makeIPv4Candidate(
            foundation: "2",
            ip: "192.168.1.2",
            port: 1_002
        )

        await sut.append(.candidates([firstCandidate]))
        await sut.append(.candidates([secondCandidate]))
        await sut.performFlushBuffer(with: 1)

        let sentSignals = signaler.sentSignals
        let expectedSignals: [PeerConnectionSignal] = [
            .candidates([firstCandidate])
        ]

        #expect(sentSignals == expectedSignals)
        #expect(signaler.sentBatches == [expectedSignals])
    }

    @Test("Flush preserves setup signal ordering")
    static func flushPreservesSetupSignalOrdering() async throws {
        let signaler = MockPeerConnectionSignaler()
        let sut = DataConnectionContext(
            signaler: signaler,
            logger: MockLogger()
        )
        let firstCandidate = SdpCoderTests.makeIPv4Candidate(
            foundation: "1",
            ip: "192.168.1.1",
            port: 1_001
        )
        let secondCandidate = SdpCoderTests.makeIPv4Candidate(
            foundation: "2",
            ip: "192.168.1.2",
            port: 1_002
        )
        let answer = PeerConnectionSignal.answer(SdpCoderTests.validAnswerSdp)

        await sut.append(.candidates([firstCandidate]))
        await sut.append(answer)
        await sut.append(.candidates([secondCandidate]))

        await sut.performFlushBuffer(with: 3)

        let sentSignals = signaler.sentSignals
        let expectedSignals: [PeerConnectionSignal] = [
            .candidates([firstCandidate]),
            answer,
            .candidates([secondCandidate])
        ]

        #expect(sentSignals == expectedSignals)
        #expect(signaler.sentBatches == [expectedSignals])
    }

    @Test("Autoflush restarts after send cancellation")
    static func autoflushRestartsAfterSendCancellation() async throws {
        let sleeper = ManualSleeper()
        let signaler = MockPeerConnectionSignaler(sendErrors: [CancellationError()])
        let sut = DataConnectionContext(
            signaler: signaler,
            logger: MockLogger(),
            sleep: { delay in
                try await sleeper.sleep(for: delay)
            }
        )
        let firstCandidate = SdpCoderTests.makeIPv4Candidate(
            foundation: "1",
            ip: "192.168.1.1",
            port: 1_001
        )
        let secondCandidate = SdpCoderTests.makeIPv4Candidate(
            foundation: "2",
            ip: "192.168.1.2",
            port: 1_002
        )

        var sendAttempts = signaler.sendAttempts.makeAsyncIterator()

        await sut.startAutoflush(limitPerFlush: 1)
        await sut.append(.candidates([firstCandidate]))
        await sleeper.advance()

        let firstAttempt = try #require(await sendAttempts.next())
        #expect(firstAttempt == [.candidates([firstCandidate])])

        await sut.append(.candidates([secondCandidate]))
        await sleeper.advance()

        let expectedSignal = PeerConnectionSignal.candidates([secondCandidate])
        let secondAttempt = try #require(await sendAttempts.next())
        #expect(secondAttempt == [expectedSignal])
        #expect(signaler.sentSignals == [expectedSignal])
    }
}
