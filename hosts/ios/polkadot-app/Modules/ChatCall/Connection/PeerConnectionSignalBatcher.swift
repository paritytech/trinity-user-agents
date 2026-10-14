protocol PeerConnectionSignalBatching {
    func batch(_ signals: [PeerConnectionSignal]) -> [BatchedPeerConnectionSignal]
}

enum BatchedPeerConnectionSignal: Equatable {
    case offer(SdpCoderSetup)
    case answer(SdpCoderSetup)
    case candidates([PeerConnectionCandidate])
    case closed
}

struct PeerConnectionSignalBatcher: PeerConnectionSignalBatching {
    static let defaultMaxCandidatesPerBatch = 12
    static let defaultMaxCandidatesPerSetup = 6

    let maxCandidatesPerBatch: Int
    let maxCandidatesPerSetup: Int
    let logger: LoggerProtocol

    init(
        maxCandidatesPerBatch: Int = Self.defaultMaxCandidatesPerBatch,
        maxCandidatesPerSetup: Int = Self.defaultMaxCandidatesPerSetup,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.maxCandidatesPerBatch = max(1, maxCandidatesPerBatch)
        self.maxCandidatesPerSetup = max(1, maxCandidatesPerSetup)
        self.logger = logger
    }

    func batch(_ signals: [PeerConnectionSignal]) -> [BatchedPeerConnectionSignal] {
        var batches: [BatchedPeerConnectionSignal] = []
        var stats = BatchingStats()

        for signal in signals {
            switch signal {
            case let .offer(sdp):
                batches.append(.offer(SdpCoderSetup(setupSdp: sdp, candidates: [])))
            case let .answer(sdp):
                batches.append(.answer(SdpCoderSetup(setupSdp: sdp, candidates: [])))
            case let .candidates(candidates):
                append(candidates, to: &batches, stats: &stats)
            case .closed:
                batches.append(.closed)
            }
        }

        logger.debug(
            "Batched \(signals.count) peer signal(s) into \(batches.count) message(s), " +
                "setupCandidates=\(stats.setupCandidates), " +
                "candidateBatches=\(stats.candidateBatches), " +
                "candidateCount=\(stats.candidateCount)"
        )

        return batches
    }
}

private struct BatchingStats {
    var setupCandidates = 0
    var candidateBatches = 0
    var candidateCount = 0
}

private extension PeerConnectionSignalBatcher {
    func append(
        _ candidates: [PeerConnectionCandidate],
        to batches: inout [BatchedPeerConnectionSignal],
        stats: inout BatchingStats
    ) {
        var remaining = candidates

        if let lastBatch = batches.last {
            switch lastBatch {
            case let .offer(setup):
                let updated = setupByAppendingCandidates(from: &remaining, to: setup)
                batches[batches.count - 1] = .offer(updated)
                stats.setupCandidates += updated.candidates.count - setup.candidates.count
            case let .answer(setup):
                let updated = setupByAppendingCandidates(from: &remaining, to: setup)
                batches[batches.count - 1] = .answer(updated)
                stats.setupCandidates += updated.candidates.count - setup.candidates.count
            case let .candidates(existing):
                let updated = candidatesByAppendingCandidates(from: &remaining, to: existing)
                batches[batches.count - 1] = .candidates(updated)
                stats.candidateCount += updated.count - existing.count
            case .closed:
                break
            }
        }

        appendCandidateBatches(remaining, to: &batches, stats: &stats)
    }

    func setupByAppendingCandidates(
        from remaining: inout [PeerConnectionCandidate],
        to setup: SdpCoderSetup
    ) -> SdpCoderSetup {
        let freeSlots = maxCandidatesPerSetup - setup.candidates.count

        guard freeSlots > 0 else {
            return setup
        }

        let candidatesToAppend = Array(remaining.prefix(freeSlots))
        remaining.removeFirst(candidatesToAppend.count)

        return SdpCoderSetup(
            setupSdp: setup.setupSdp,
            candidates: setup.candidates + candidatesToAppend
        )
    }

    func candidatesByAppendingCandidates(
        from remaining: inout [PeerConnectionCandidate],
        to existing: [PeerConnectionCandidate]
    ) -> [PeerConnectionCandidate] {
        let freeSlots = maxCandidatesPerBatch - existing.count

        guard freeSlots > 0 else {
            return existing
        }

        let candidatesToAppend = Array(remaining.prefix(freeSlots))
        remaining.removeFirst(candidatesToAppend.count)

        return existing + candidatesToAppend
    }

    func appendCandidateBatches(
        _ candidates: [PeerConnectionCandidate],
        to batches: inout [BatchedPeerConnectionSignal],
        stats: inout BatchingStats
    ) {
        var offset = 0

        while offset < candidates.count {
            let upperBound = min(offset + maxCandidatesPerBatch, candidates.count)
            batches.append(.candidates(Array(candidates[offset ..< upperBound])))
            stats.candidateBatches += 1
            stats.candidateCount += upperBound - offset
            offset = upperBound
        }
    }
}
