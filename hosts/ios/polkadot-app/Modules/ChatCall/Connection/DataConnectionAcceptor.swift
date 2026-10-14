import Foundation
import WebRTC
import AsyncExtensions
import AsyncAlgorithms

final class DataConnectionAcceptor: DataConnectionCreator {
    private let acceptorContext = Context()

    override func stopNegotiation() async {
        await super.stopNegotiation()
        await acceptorContext.stopNegotiation()
    }

    override func closeResources() async {
        await acceptorContext.closeResources()
        await super.closeResources()
    }
}

private extension DataConnectionAcceptor {
    func processSignaling(on wrapper: AsyncPeerConnectionWrapper) async {
        let task = Task { [weak self, context, logger] in
            let incomingSignals = await context.signals

            do {
                for try await signal in incomingSignals {
                    guard !Task.isCancelled else {
                        return
                    }

                    switch signal {
                    case let .offer(sdp):
                        logger.debug("Offer received: \(sdp.count)")

                        // Create remote description
                        let remoteSdp = RTCSessionDescription(type: .offer, sdp: sdp)
                        try await wrapper.setRemoteDescription(remoteSdp)
                        try Task.checkCancellation()

                        await self?.drainPendingRemoteCandidates(on: wrapper)
                        try Task.checkCancellation()

                        logger.debug("Set remote offer")

                        // Create initial answer (data channel only)
                        let constraints = RTCMediaConstraints(
                            mandatoryConstraints: nil,
                            optionalConstraints: nil
                        )
                        let answer = try await wrapper.answer(for: constraints)
                        try Task.checkCancellation()

                        try await wrapper.setLocalDescription(answer)
                        try Task.checkCancellation()

                        logger.debug("Set local answer")

                        await context.sendSignalAndFlushBuffer(.answer(answer.sdp))
                        try Task.checkCancellation()

                        await context.startAutoflush()

                        logger.debug("Sent answer \(answer.sdp.count)")

                    case .answer:
                        logger.error("Unexpected answer received by acceptor")

                    case let .candidates(sdpList):
                        await self?.handleRemoteCandidates(sdpList, on: wrapper)

                    case .closed:
                        await self?.clearPendingRemoteCandidates()
                        logger.debug("Remote closed signal received")
                        return
                    }
                }

                logger.debug("Signaling processing completed")
            } catch is CancellationError {
                return
            } catch {
                logger.error("Data connection signaling failed: \(error)")
                await self?.cancel()
            }
        }

        await acceptorContext.setSignalingTask(task)
    }

    func createStateSequence(
        for connectionWrapper: AsyncPeerConnectionWrapper
    ) -> AnyAsyncSequence<PeerDataConnectionState> {
        let signalingState = connectionWrapper.signalingState.eraseToAnyAsyncSequence()
        let iceState = connectionWrapper.iceConnectionState.eraseToAnyAsyncSequence()
        let openedDataChannels = connectionWrapper.openedDataChannels.eraseToAnyAsyncSequence()

        return combineLatest(
            combineLatest(signalingState, iceState),
            openedDataChannels
        )
        .map { [logger] connectionStates, dataChannels in
            let (optSignalingState, optIceState) = connectionStates

            logger.debug("Signaling: \(String(describing: optSignalingState))")
            logger.debug("ICE: \(String(describing: optIceState))")
            logger.debug("Data channels: \(dataChannels.count)")

            if let optIceState, optIceState.isTerminal {
                return .disconnected
            }

            switch optSignalingState {
            case .stable:
                if let dataChannelWrapper = dataChannels.first {
                    let model = PeerDataConnectionState.Connected(
                        connection: connectionWrapper,
                        dataChannel: dataChannelWrapper
                    )

                    return .connected(model)
                } else {
                    return .connecting
                }
            case .closed:
                return .disconnected
            default:
                return .connecting
            }
        }
        .eraseToAnyAsyncSequence()
    }
}

extension DataConnectionAcceptor: DataConnectionCreating {
    func connect() async throws -> AnyAsyncSequence<PeerDataConnectionState> {
        let connectionWrapper = try await setupConnection()

        await processSignaling(on: connectionWrapper)
        await processLocalCandidates(from: connectionWrapper)

        return createStateSequence(for: connectionWrapper)
    }
}

private extension DataConnectionAcceptor {
    actor Context {
        private var signalingTask: Task<Void, Never>?
        private var isNegotiationStopped = false
        private var isClosingResources = false

        deinit {
            signalingTask?.cancel()
        }

        func setSignalingTask(_ task: Task<Void, Never>) {
            guard !isNegotiationStopped, !isClosingResources else {
                task.cancel()
                return
            }

            signalingTask?.cancel()
            signalingTask = task
        }

        func stopNegotiation() {
            isNegotiationStopped = true
            signalingTask?.cancel()
            signalingTask = nil
        }

        func closeResources() {
            isClosingResources = true
            stopNegotiation()
        }
    }
}
