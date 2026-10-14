import Foundation
import WebRTC
import AsyncExtensions
import AsyncAlgorithms

enum CallCreatorError: Error {
    case negotiationFailed
    case iceConnectionFailed
}

protocol CallCreatorProtocol: AnyObject {
    var multiplexedChannel: MultiplexedDataChannel { get }

    func setup()
    func throttle()
    func subscribeState() -> AnyAsyncSequence<CallCreationState>
}

class CallCreator {
    let connectionWrapper: AsyncPeerConnectionWrapper
    let dataChannelWrapper: AsyncDataChannelWrapper
    let multiplexedChannel: MultiplexedDataChannel
    let localTracks: CallTracks
    let transceiverConfigStrategy: TransceiverConfigStrategyProtocol
    let logger: LoggerProtocol

    let signaling: PeerConnectionSignaling

    let negotiated = AsyncCurrentValueSubject<Bool?>(nil)

    private let candidateFilter: ConnectionCandidateFiltering?

    private var candidatesTask: Task<Void, Never>?
    private let pendingRemoteCandidates: PendingRemoteCandidatesBuffering

    init(
        connectionWrapper: AsyncPeerConnectionWrapper,
        dataChannelWrapper: AsyncDataChannelWrapper,
        localTracks: CallTracks,
        transceiverConfigStrategy: TransceiverConfigStrategyProtocol,
        candidateFilter: ConnectionCandidateFiltering? = nil,
        logger: LoggerProtocol,
        pendingRemoteCandidates: PendingRemoteCandidatesBuffering = PendingRemoteCandidatesBuffer(
            label: "CallCreator"
        )
    ) {
        self.connectionWrapper = connectionWrapper
        self.dataChannelWrapper = dataChannelWrapper
        self.localTracks = localTracks
        self.candidateFilter = candidateFilter
        self.logger = logger
        self.transceiverConfigStrategy = transceiverConfigStrategy
        self.pendingRemoteCandidates = pendingRemoteCandidates

        multiplexedChannel = MultiplexedDataChannel(
            dataChannelWrapper: dataChannelWrapper,
            logger: logger
        )
        multiplexedChannel.start()

        signaling = DataChannelSignaler(
            multiplexedChannel: multiplexedChannel,
            logger: logger
        )
    }

    deinit {
        logger.debug("Deinited")
    }

    func processLocalCandidates(from wrapper: AsyncPeerConnectionWrapper) {
        candidatesTask = Task { [signaling, logger, candidateFilter] in
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
                        try await signaling.send([.candidates([signalCandidate])])
                        logger.debug("Sent new candidate: \(signalCandidate)")
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

    private func filterIncomingCandidates(
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

    func applyLocalTracks() async {
        do {
            try await connectionWrapper.modify { [localTracks, transceiverConfigStrategy] connection in
                if let audioTrack = localTracks.audioTrack {
                    connection.add(audioTrack, streamIds: ["stream0"])
                }

                transceiverConfigStrategy.configureVideoTransceiver(
                    for: localTracks.videoTrack,
                    on: connection
                )
            }
        } catch {
            logger.error("Failed to apply local tracks: \(error)")
        }
    }

    func stopNegotiation() {
        candidatesTask?.cancel()
        candidatesTask = nil
    }

    func throttle() {
        stopNegotiation()
    }
}

private extension CallCreator {
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

extension CallCreator {
    func subscribeState() -> AnyAsyncSequence<CallCreationState> {
        let negotiatedStream = negotiated.eraseToAnyAsyncSequence()
        let signalingStateStream = connectionWrapper.signalingState.eraseToAnyAsyncSequence()
        let iceStateStream = connectionWrapper.iceConnectionState.eraseToAnyAsyncSequence()
        let tracksStream = connectionWrapper
            .rtpReceivers
            .map { rtpReceivers in
                rtpReceivers.reduce(CallTracks()) { $0.replacingFromRTPReceiver($1) }
            }
            .eraseToAnyAsyncSequence()

        return combineLatest(
            combineLatest(signalingStateStream, iceStateStream),
            combineLatest(negotiatedStream, tracksStream)
        ).map { connectionStates, negotiationStates in
            let (signalingState, iceState) = connectionStates
            let (negotiated, tracks) = negotiationStates

            if let iceState, iceState.isTerminal {
                let error = CallCreatorError(iceState: iceState)
                return CallCreationState.closed(error)
            }

            guard negotiated != false else {
                return CallCreationState.closed(CallCreatorError.negotiationFailed)
            }

            guard signalingState == .stable, negotiated == true else {
                return CallCreationState.creating
            }

            return CallCreationState.ready(tracks)
        }
        .eraseToAnyAsyncSequence()
    }
}

extension CallCreatorError {
    init?(iceState: RTCIceConnectionState) {
        switch iceState {
        case .failed:
            self = .iceConnectionFailed
        default:
            return nil
        }
    }
}
