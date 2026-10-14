import Foundation
import WebRTC
import AsyncExtensions
import StructuredConcurrency

enum AsyncPeerConnectionWrapperError: Error {
    case closed
    case dataChannelCreationFailed
    case peerConnectionCreationFailed
}

/// Wraps `RTCPeerConnection` and serialises both *outbound operations* and
/// *inbound delegate events* through a single `SerialOperationQueue`.
///
/// - Operations (`setLocalDescription`, `addRemoteCandidate`, `close`, …)
///   are submitted via `queue.run` directly by callers.
/// - Delegate callbacks fire on WebRTC's signalling thread, yield a
///   `DelegateEvent` into an internal `AsyncStream`, and a dedicated
///   consumer task drains that stream — each event applied via the same
///   `queue.run`. As a result, subject updates (e.g. `signalingState`)
///   never race with operations on the underlying connection.
///
/// The same queue is passed down to every child `AsyncDataChannelWrapper`
/// (both initiator-created and delegate-received) so peer-connection ops
/// and data-channel sends share one serialisation domain.
///
/// INVARIANT: `connection` and `lifecycle` are read/written only inside
/// `queue.run { ... }` blocks. Delegate callbacks do not touch them —
/// they only yield to the event stream.
final class AsyncPeerConnectionWrapper: NSObject {
    enum CandidateState {
        case add(RTCIceCandidate)
        case remove([RTCIceCandidate])
    }

    private enum LifecycleState {
        case open
        case closing
        case closed
    }

    private enum DelegateEvent {
        case signalingState(RTCSignalingState)
        case iceConnectionState(RTCIceConnectionState)
        case iceGatheringState(RTCIceGatheringState)
        case shouldNegotiate
        case rtpReceiverAdded(RTCRtpReceiver)
        case rtpReceiverRemoved(RTCRtpReceiver)
        case candidateGenerated(RTCIceCandidate)
        case candidatesRemoved([RTCIceCandidate])
        case dataChannelOpened(AsyncDataChannelWrapper)
    }

    private let connection: RTCPeerConnection
    let logger: LoggerProtocol

    private let queue = SerialOperationQueue()

    private let delegateEvents: AsyncStream<DelegateEvent>
    private let delegateContinuation: AsyncStream<DelegateEvent>.Continuation
    private var delegateTask: Task<Void, Never>?

    let signalingState: AsyncCurrentValueSubject<RTCSignalingState?> = .init(nil)
    let rtpReceivers: AsyncCurrentValueSubject<Set<RTCRtpReceiver>> = .init([])
    let shouldNegotiateState: AsyncCurrentValueSubject<Bool> = .init(false)
    let openedDataChannels: AsyncCurrentValueSubject<[AsyncDataChannelWrapper]> = .init([])
    let iceConnectionState: AsyncCurrentValueSubject<RTCIceConnectionState?> = .init(nil)
    let iceGatheringState: AsyncCurrentValueSubject<RTCIceGatheringState?> = .init(nil)
    let candidates: AsyncPassthroughSubject<CandidateState> = .init()

    private var lifecycle: LifecycleState = .open

    init(
        connection: RTCPeerConnection,
        logger: LoggerProtocol
    ) {
        self.logger = logger
        self.connection = connection

        let (stream, continuation) = AsyncStream<DelegateEvent>.makeStream(bufferingPolicy: .unbounded)
        delegateEvents = stream
        delegateContinuation = continuation

        super.init()

        connection.delegate = self
        startDelegateConsumer()
    }

    deinit {
        delegateContinuation.finish()
        delegateTask?.cancel()
        logger.debug("Deinited")
    }

    // MARK: - Serialised WebRTC operations

    func setLocalDescription(_ sdp: RTCSessionDescription) async throws {
        try await queue.run { [self] in
            try requireOpen()

            logger.debug("Setting local description")

            try await connection.setLocalDescription(sdp)

            logger.debug("Local description set")
        }
    }

    func setRemoteDescription(_ sdp: RTCSessionDescription) async throws {
        try await queue.run { [self] in
            try requireOpen()

            logger.debug("Setting remote description")

            try await connection.setRemoteDescription(sdp)

            logger.debug("Remote description is set")
        }
    }

    func addRemoteCandidate(_ candidate: RTCIceCandidate) async throws {
        try await queue.run { [self] in
            try requireOpen()

            logger.debug("Adding candidate")

            try await connection.add(candidate)

            logger.debug("Candidate added")
        }
    }

    func offer(for constraints: RTCMediaConstraints) async throws -> RTCSessionDescription {
        try await queue.run { [self] in
            try requireOpen()

            logger.debug("Creating offer")

            let description = try await connection.offer(for: constraints)

            logger.debug("Offer created")

            return description
        }
    }

    func answer(for constraints: RTCMediaConstraints) async throws -> RTCSessionDescription {
        try await queue.run { [self] in
            try requireOpen()

            logger.debug("Creating answer")

            let description = try await connection.answer(for: constraints)

            logger.debug("Answer created")

            return description
        }
    }

    func createDataChannel(
        label: String,
        configuration: RTCDataChannelConfiguration
    ) async throws -> AsyncDataChannelWrapper {
        try await queue.run { [self] in
            try requireOpen()

            logger.debug("Creating data channel")

            guard let dataChannel = connection.dataChannel(forLabel: label, configuration: configuration) else {
                throw AsyncPeerConnectionWrapperError.dataChannelCreationFailed
            }

            logger.debug("Data channel created")

            return AsyncDataChannelWrapper(dataChannel: dataChannel, queue: queue, logger: logger)
        }
    }

    func modify(_ changeClosure: @escaping (RTCPeerConnection) async throws -> Void) async throws {
        try await queue.run { [self] in
            try requireOpen()

            logger.debug("Modifing connection")

            try await changeClosure(connection)

            logger.debug("Connection modified")
        }
    }

    // MARK: - Serialised state reads

    //
    // These exist so callers don't read `connection.*` directly, which would
    // race with writes already enqueued but not yet executed. Reads are
    // queued behind pending writes, so they observe a consistent state.
    // After the wrapper is closed they return safe defaults (.closed / false)
    // instead of throwing — reads on a closed wrapper are not an error.

    func hasRemoteDescription() async -> Bool {
        await (try? queue.run { [self] in
            try requireOpen()
            return connection.remoteDescription != nil
        }) ?? false
    }

    func currentSignalingState() async -> RTCSignalingState {
        await (try? queue.run { [self] in
            try requireOpen()
            return connection.signalingState
        }) ?? .closed
    }

    func close() async {
        await queue.run { [self] in
            await performClose()
        }
        // Drain the delegate consumer. Any events still buffered will be
        // applied as no-ops thanks to the lifecycle guard in
        // `applyDelegateEvent`.
        delegateContinuation.finish()
    }

    // MARK: - Private (queue-isolated)

    private func performClose() async {
        guard lifecycle == .open else { return }
        lifecycle = .closing

        logger.debug("Closing connection")

        connection.delegate = nil
        connection.close()
        signalingState.send(.closed)
        iceConnectionState.send(.closed)
        lifecycle = .closed

        logger.debug("Connection closed")
    }

    private func requireOpen() throws {
        switch lifecycle {
        case .open:
            return
        case .closing,
             .closed:
            throw AsyncPeerConnectionWrapperError.closed
        }
    }

    // MARK: - Delegate event pipeline

    private func startDelegateConsumer() {
        delegateTask = Task { [weak self, delegateEvents, queue] in
            for await event in delegateEvents {
                await queue.run { [weak self] in
                    self?.applyDelegateEvent(event)
                }
            }
        }
    }

    /// Runs on the shared serial queue. Lifecycle is mutated only on this
    /// queue, so checking it here is race-free. Events that arrive after
    /// close are dropped — publishing stale subject values on a torn-down
    /// connection would only confuse observers.
    private func applyDelegateEvent(_ event: DelegateEvent) {
        guard lifecycle == .open else { return }

        switch event {
        case let .signalingState(state):
            signalingState.send(state)
        case let .iceConnectionState(state):
            iceConnectionState.send(state)
        case let .iceGatheringState(state):
            iceGatheringState.send(state)
        case .shouldNegotiate:
            shouldNegotiateState.send(true)
        case let .rtpReceiverAdded(receiver):
            var receivers = rtpReceivers.value
            receivers.insert(receiver)
            rtpReceivers.send(receivers)
        case let .rtpReceiverRemoved(receiver):
            var receivers = rtpReceivers.value
            receivers.remove(receiver)
            rtpReceivers.send(receivers)
        case let .candidateGenerated(candidate):
            candidates.send(.add(candidate))
        case let .candidatesRemoved(removed):
            candidates.send(.remove(removed))
        case let .dataChannelOpened(wrapper):
            var channels = openedDataChannels.value
            channels.append(wrapper)
            openedDataChannels.send(channels)
        }
    }
}

extension AsyncPeerConnectionWrapper: RTCPeerConnectionDelegate {
    // legacy api still required but we handle stream as rtpReceivers
    func peerConnection(_: RTCPeerConnection, didRemove _: RTCMediaStream) {}
    func peerConnection(_: RTCPeerConnection, didAdd _: RTCMediaStream) {}

    func peerConnection(_: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {
        logger.debug("Did change signaling state: \(stateChanged)")
        delegateContinuation.yield(.signalingState(stateChanged))
    }

    func peerConnectionShouldNegotiate(_: RTCPeerConnection) {
        delegateContinuation.yield(.shouldNegotiate)
    }

    func peerConnection(_: RTCPeerConnection, didAdd rtpReceiver: RTCRtpReceiver, streams _: [RTCMediaStream]) {
        logger.debug("Did add rtp receiver")
        delegateContinuation.yield(.rtpReceiverAdded(rtpReceiver))
    }

    func peerConnection(_: RTCPeerConnection, didRemove rtpReceiver: RTCRtpReceiver) {
        logger.debug("Did remove rtp receiver")
        delegateContinuation.yield(.rtpReceiverRemoved(rtpReceiver))
    }

    func peerConnection(_: RTCPeerConnection, didChange newState: RTCIceConnectionState) {
        logger.debug("Did change ice connection state: \(newState)")
        delegateContinuation.yield(.iceConnectionState(newState))
    }

    func peerConnection(_: RTCPeerConnection, didChange newState: RTCIceGatheringState) {
        logger.debug("Did change ice gathering state: \(newState)")
        delegateContinuation.yield(.iceGatheringState(newState))
    }

    func peerConnection(_: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {
        logger.debug("Did generate candidate")
        delegateContinuation.yield(.candidateGenerated(candidate))
    }

    func peerConnection(_: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {
        logger.debug("Did remove candidates: \(candidates.count)")
        delegateContinuation.yield(.candidatesRemoved(candidates))
    }

    func peerConnection(_: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {
        logger.debug("Did open data channel")

        // Construct the wrapper synchronously on WebRTC's signalling
        // thread so its delegate is attached before any inbound message
        // can be delivered. Bookkeeping (appending to openedDataChannels)
        // flows through the queue via the consumer task.
        let wrapper = AsyncDataChannelWrapper(dataChannel: dataChannel, queue: queue, logger: logger)
        delegateContinuation.yield(.dataChannelOpened(wrapper))
    }
}
