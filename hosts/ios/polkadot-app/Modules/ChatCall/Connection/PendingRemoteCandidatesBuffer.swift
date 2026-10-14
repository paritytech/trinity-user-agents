import Foundation

protocol PendingRemoteCandidatesBuffering {
    func append(_ newCandidates: [PeerConnectionCandidate]) async
    func takeAll() async -> [PeerConnectionCandidate]
    func clear() async
}

actor PendingRemoteCandidatesBuffer: PendingRemoteCandidatesBuffering {
    private static let defaultLimit = 64

    private let label: String
    private let logger: LoggerProtocol
    private let limit: Int
    private var candidates: [PeerConnectionCandidate] = []

    init(
        label: String,
        logger: LoggerProtocol = Logger.shared,
        limit: Int = PendingRemoteCandidatesBuffer.defaultLimit
    ) {
        self.label = label
        self.logger = logger
        self.limit = limit
    }

    func append(_ newCandidates: [PeerConnectionCandidate]) {
        guard !newCandidates.isEmpty else { return }

        candidates.append(contentsOf: newCandidates)

        let droppedCount = max(candidates.count - limit, 0)
        if droppedCount > 0 {
            candidates.removeFirst(droppedCount)
        }

        logger.debug(
            "[\(label)] Buffered \(newCandidates.count) remote candidate(s) until remote description is set"
        )

        guard droppedCount > 0 else { return }

        logger.warning(
            "[\(label)] Dropped \(droppedCount) oldest pending remote candidate(s); " +
                "buffer size is \(candidates.count)"
        )
    }

    func takeAll() -> [PeerConnectionCandidate] {
        let result = candidates
        candidates.removeAll()

        if !result.isEmpty {
            logger.debug("[\(label)] Applying \(result.count) pending remote candidate(s)")
        }

        return result
    }

    func clear() {
        let count = candidates.count
        candidates.removeAll()

        if count > 0 {
            logger.debug("[\(label)] Dropping \(count) pending remote candidate(s)")
        }
    }
}
