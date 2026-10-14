@testable import polkadot_app
import Testing

enum PeerConnectionSignalBatcherTests {
    @Test("Attaches following candidates to offer setup")
    static func attachesFollowingCandidatesToOfferSetup() {
        let sut = makeSut()
        let candidates = makeCandidates(count: 2)

        let batches = sut.batch([
            .offer(SdpCoderTests.validOfferSdp),
            .candidates([candidates[0]]),
            .candidates([candidates[1]])
        ])

        #expect(batches == [
            .offer(SdpCoderSetup(setupSdp: SdpCoderTests.validOfferSdp, candidates: candidates))
        ])
    }

    @Test("Attaches following candidates to answer setup")
    static func attachesFollowingCandidatesToAnswerSetup() {
        let sut = makeSut()
        let candidates = makeCandidates(count: 2)

        let batches = sut.batch([
            .answer(SdpCoderTests.validAnswerSdp),
            .candidates([candidates[0]]),
            .candidates([candidates[1]])
        ])

        #expect(batches == [
            .answer(SdpCoderSetup(setupSdp: SdpCoderTests.validAnswerSdp, candidates: candidates))
        ])
    }

    @Test("Keeps candidates before setup standalone")
    static func keepsCandidatesBeforeSetupStandalone() {
        let sut = makeSut()
        let candidates = makeCandidates(count: 2)

        let batches = sut.batch([
            .candidates([candidates[0]]),
            .offer(SdpCoderTests.validOfferSdp),
            .candidates([candidates[1]])
        ])

        #expect(batches == [
            .candidates([candidates[0]]),
            .offer(SdpCoderSetup(setupSdp: SdpCoderTests.validOfferSdp, candidates: [candidates[1]]))
        ])
    }

    @Test("Uses default setup candidate limit")
    static func usesDefaultSetupCandidateLimit() {
        let sut = makeSut()
        let candidates = makeCandidates(count: PeerConnectionSignalBatcher.defaultMaxCandidatesPerSetup + 1)
        let setupLimit = PeerConnectionSignalBatcher.defaultMaxCandidatesPerSetup

        let batches = sut.batch([
            .offer(SdpCoderTests.validOfferSdp),
            .candidates(candidates)
        ])

        #expect(batches == [
            .offer(SdpCoderSetup(
                setupSdp: SdpCoderTests.validOfferSdp,
                candidates: Array(candidates[0 ..< setupLimit])
            )),
            .candidates(Array(candidates[setupLimit ..< candidates.count]))
        ])
    }

    @Test("Applies setup candidate limit across multiple candidate signals")
    static func appliesSetupCandidateLimitAcrossMultipleCandidateSignals() {
        let sut = PeerConnectionSignalBatcher(
            maxCandidatesPerBatch: 12,
            maxCandidatesPerSetup: 6,
            logger: MockLogger()
        )
        let candidates = makeCandidates(count: 10)

        let batches = sut.batch([
            .offer(SdpCoderTests.validOfferSdp),
            .candidates(Array(candidates[0 ..< 5])),
            .candidates(Array(candidates[5 ..< 10]))
        ])

        #expect(batches == [
            .offer(SdpCoderSetup(
                setupSdp: SdpCoderTests.validOfferSdp,
                candidates: Array(candidates[0 ..< 6])
            )),
            .candidates(Array(candidates[6 ..< 10]))
        ])
    }

    @Test("Uses default standalone candidate batch limit")
    static func usesDefaultStandaloneCandidateBatchLimit() {
        let sut = makeSut()
        let batchLimit = PeerConnectionSignalBatcher.defaultMaxCandidatesPerBatch
        let candidates = makeCandidates(count: batchLimit * 2 + 1)

        let batches = sut.batch([
            .candidates(candidates)
        ])

        #expect(batches == [
            .candidates(Array(candidates[0 ..< batchLimit])),
            .candidates(Array(candidates[batchLimit ..< batchLimit * 2])),
            .candidates(Array(candidates[batchLimit * 2 ..< candidates.count]))
        ])
    }

    @Test("Merges consecutive candidate signals up to standalone limit")
    static func mergesConsecutiveCandidateSignalsUpToStandaloneLimit() {
        let sut = PeerConnectionSignalBatcher(
            maxCandidatesPerBatch: 4,
            maxCandidatesPerSetup: 2,
            logger: MockLogger()
        )
        let candidates = makeCandidates(count: 6)

        let batches = sut.batch([
            .candidates(Array(candidates[0 ..< 2])),
            .candidates(Array(candidates[2 ..< 5])),
            .candidates(Array(candidates[5 ..< 6]))
        ])

        #expect(batches == [
            .candidates(Array(candidates[0 ..< 4])),
            .candidates(Array(candidates[4 ..< 6]))
        ])
    }

    @Test("Splits setup overflow using standalone batch limit")
    static func splitsSetupOverflowUsingStandaloneBatchLimit() {
        let sut = PeerConnectionSignalBatcher(
            maxCandidatesPerBatch: 3,
            maxCandidatesPerSetup: 2,
            logger: MockLogger()
        )
        let candidates = makeCandidates(count: 6)

        let batches = sut.batch([
            .offer(SdpCoderTests.validOfferSdp),
            .candidates(candidates)
        ])

        #expect(batches == [
            .offer(SdpCoderSetup(setupSdp: SdpCoderTests.validOfferSdp, candidates: Array(candidates[0 ..< 2]))),
            .candidates(Array(candidates[2 ..< 5])),
            .candidates(Array(candidates[5 ..< 6]))
        ])
    }

    @Test("Empty input returns empty output")
    static func emptyInputReturnsEmptyOutput() {
        let sut = makeSut()

        let batches = sut.batch([])

        #expect(batches.isEmpty)
    }

    @Test("Does not attach candidates across closed boundary")
    static func doesNotAttachCandidatesAcrossClosedBoundary() {
        let sut = makeSut()
        let candidates = makeCandidates(count: 2)

        let batches = sut.batch([
            .offer(SdpCoderTests.validOfferSdp),
            .candidates([candidates[0]]),
            .closed,
            .candidates([candidates[1]])
        ])

        #expect(batches == [
            .offer(SdpCoderSetup(setupSdp: SdpCoderTests.validOfferSdp, candidates: [candidates[0]])),
            .closed,
            .candidates([candidates[1]])
        ])
    }

    @Test("Attaches candidates only to nearest setup")
    static func attachesCandidatesOnlyToNearestSetup() {
        let sut = makeSut()
        let candidates = makeCandidates(count: 2)

        let batches = sut.batch([
            .offer(SdpCoderTests.validOfferSdp),
            .candidates([candidates[0]]),
            .answer(SdpCoderTests.validAnswerSdp),
            .candidates([candidates[1]])
        ])

        #expect(batches == [
            .offer(SdpCoderSetup(setupSdp: SdpCoderTests.validOfferSdp, candidates: [candidates[0]])),
            .answer(SdpCoderSetup(setupSdp: SdpCoderTests.validAnswerSdp, candidates: [candidates[1]]))
        ])
    }
}

private func makeSut() -> PeerConnectionSignalBatcher {
    PeerConnectionSignalBatcher(logger: MockLogger())
}

private func makeCandidates(count: Int) -> [PeerConnectionCandidate] {
    (1 ... count).map { index in
        SdpCoderTests.makeIPv4Candidate(
            foundation: "\(index)",
            ip: "192.168.1.\(index)",
            port: UInt16(1_000 + index)
        )
    }
}
