import Foundation
import AsyncExtensions
import WebRTC

enum DataConnectionCreatorError: Error {
    case connectionCreationFailed
}

protocol DataConnectionCreating {
    func connect() async throws -> AnyAsyncSequence<PeerDataConnectionState>
    func cancel() async
    func throttle() async
}

class DataConnectionCreator {
    let logger: LoggerProtocol

    let peerConnectionFactory: RTCPeerConnectionFactory
    let configFactory: WebRTCConfigMaking
    let context: DataConnectionContext

    private let candidateFilter: ConnectionCandidateFiltering?

    private let lifecycleContext = Context()
    private let pendingRemoteCandidates: PendingRemoteCandidatesBuffering

    init(
        signaling: PeerConnectionSignaling,
        peerConnectionFactory: RTCPeerConnectionFactory,
        configFactory: WebRTCConfigMaking,
        candidateFilter: ConnectionCandidateFiltering? = nil,
        logger: LoggerProtocol,
        pendingRemoteCandidates: PendingRemoteCandidatesBuffering = PendingRemoteCandidatesBuffer(
            label: "DataConnectionCreator"
        )
    ) {
        context = DataConnectionContext(
            signaler: signaling,
            logger: logger
        )

        self.peerConnectionFactory = peerConnectionFactory
        self.configFactory = configFactory
        self.candidateFilter = candidateFilter
        self.logger = logger
        self.pendingRemoteCandidates = pendingRemoteCandidates
    }

    deinit {
        logger.debug("Deinited")
    }

    func setupConnection() async throws -> AsyncPeerConnectionWrapper {
        let configuration = try await configFactory.makeConnectionConfiguration()

        let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)

        guard let connection = peerConnectionFactory.peerConnection(
            with: configuration,
            constraints: constraints,
            delegate: nil
        ) else {
            throw AsyncPeerConnectionWrapperError.peerConnectionCreationFailed
        }

        let connectionWrapper = AsyncPeerConnectionWrapper(connection: connection, logger: logger)

        guard await lifecycleContext.setConnectionWrapper(connectionWrapper) else {
            throw CancellationError()
        }

        return connectionWrapper
    }

    func processLocalCandidates(from wrapper: AsyncPeerConnectionWrapper) async {
        let task = Task { [context, logger, candidateFilter] in
            let sequence = wrapper.candidates.eraseToAnyAsyncSequence()

            do {
                for try await candidateOp in sequence {
                    guard !Task.isCancelled else {
                        return
                    }

                    switch candidateOp {
                    case let .add(iceCandidate):
                        let signalCandidate = PeerConnectionCandidate(iceCandidate: iceCandidate)
                        if let candidateFilter, !candidateFilter.shouldAccept(signalCandidate) {
                            logger.debug("Filtered outgoing candidate: \(iceCandidate.sdp)")
                            continue
                        }
                        await context.append(.candidates([signalCandidate]))
                        logger.debug("Sent new candidate: \(iceCandidate.sdp)")
                    case .remove:
                        // unsupported yet
                        logger.warning("Candidate removed but this not yet supported")
                    }
                }

                logger.debug("Candidates processing completed")
            } catch {
                logger.error("Candidates task failed")
            }
        }

        await lifecycleContext.setCandidatesTask(task)
    }

    func handleRemoteCandidates(
        _ candidates: [PeerConnectionCandidate],
        on wrapper: AsyncPeerConnectionWrapper
    ) async {
        let accepted = filterIncomingCandidates(candidates)
        guard !accepted.isEmpty else { return }

        guard await wrapper.hasRemoteDescription() else {
            await bufferRemoteCandidates(accepted)
            return
        }

        await applyRemoteCandidates(accepted, on: wrapper)
    }

    func drainPendingRemoteCandidates(on wrapper: AsyncPeerConnectionWrapper) async {
        guard await wrapper.hasRemoteDescription() else {
            return
        }

        let candidates = await pendingRemoteCandidates.takeAll()
        guard !candidates.isEmpty else { return }

        await applyRemoteCandidates(candidates, on: wrapper)
    }

    func clearPendingRemoteCandidates() async {
        await pendingRemoteCandidates.clear()
    }

    func stopNegotiation() async {
        await lifecycleContext.stopNegotiation()
        await context.stopBuffering()
        await pendingRemoteCandidates.clear()
    }

    func closeResources() async {
        await lifecycleContext.closeConnection()
    }

    func cancel() async {
        await stopNegotiation()
        await closeResources()
    }

    func throttle() async {
        await stopNegotiation()
    }
}

private extension DataConnectionCreator {
    func filterIncomingCandidates(
        _ candidates: [PeerConnectionCandidate]
    ) -> [PeerConnectionCandidate] {
        guard let candidateFilter else { return candidates }
        return candidates.filter { candidate in
            guard candidateFilter.shouldAccept(candidate) else {
                logger.debug("Filtered incoming candidate: \(candidate)")
                return false
            }
            return true
        }
    }
}

private extension DataConnectionCreator {
    actor Context {
        private var connectionWrapper: AsyncPeerConnectionWrapper?
        private var candidatesTask: Task<Void, Never>?
        private var isNegotiationStopped = false
        private var isClosingResources = false

        deinit {
            candidatesTask?.cancel()
        }

        func setConnectionWrapper(_ wrapper: AsyncPeerConnectionWrapper) async -> Bool {
            guard !isClosingResources else {
                await wrapper.close()
                return false
            }

            connectionWrapper = wrapper

            return true
        }

        func setCandidatesTask(_ task: Task<Void, Never>) {
            guard !isNegotiationStopped, !isClosingResources else {
                task.cancel()
                return
            }

            candidatesTask?.cancel()
            candidatesTask = task
        }

        func stopNegotiation() {
            isNegotiationStopped = true
            candidatesTask?.cancel()
            candidatesTask = nil
        }

        func closeConnection() async {
            isClosingResources = true
            stopNegotiation()

            let wrapper = connectionWrapper
            connectionWrapper = nil

            await wrapper?.close()
        }
    }

    func bufferRemoteCandidates(_ candidates: [PeerConnectionCandidate]) async {
        guard !candidates.isEmpty else { return }

        await pendingRemoteCandidates.append(candidates)
    }

    func applyRemoteCandidates(
        _ candidates: [PeerConnectionCandidate],
        on wrapper: AsyncPeerConnectionWrapper
    ) async {
        for candidate in candidates {
            do {
                try await wrapper.addRemoteCandidate(candidate.toRTCIceCandidate())
                logger.debug("Applied candidate: \(candidate)")
            } catch {
                logger.error("Can't process candidate: \(candidate)")
            }
        }
    }
}
