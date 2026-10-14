import Foundation
import WebRTC

final class CallAcceptor: CallCreator {
    private var processingTask: Task<Void, Never>?

    override func stopNegotiation() {
        super.stopNegotiation()

        processingTask?.cancel()
        processingTask = nil
    }
}

private extension CallAcceptor {
    private func processCallOffer() {
        logger.debug("Setup signal processing")

        processingTask = Task { [weak self, connectionWrapper, signaling, logger] in
            let incomingSignals = await (signaling.signals).eraseToAnyAsyncSequence()

            do {
                for try await signal in incomingSignals {
                    logger.debug("Processing new signal")

                    guard !Task.isCancelled else {
                        return
                    }

                    switch signal {
                    case let .offer(sdp):
                        let remoteSdp = RTCSessionDescription(type: .offer, sdp: sdp)
                        try await connectionWrapper.setRemoteDescription(remoteSdp)
                        try Task.checkCancellation()

                        await self?.drainPendingRemoteCandidates(on: connectionWrapper)
                        try Task.checkCancellation()

                        logger.debug("Set remote offer successfully")

                        await self?.applyLocalTracks()
                        try Task.checkCancellation()

                        logger.debug("Media tracks set")

                        let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)

                        let answer = try await connectionWrapper.answer(for: constraints)
                        try Task.checkCancellation()

                        logger.debug("Local answer created")

                        try await connectionWrapper.setLocalDescription(answer)
                        try Task.checkCancellation()

                        logger.debug("Local answer set")

                        let sendResult = try await signaling.send([.answer(answer.sdp)])

                        guard sendResult.isFullySent else {
                            throw CallCreatorError.negotiationFailed
                        }

                        try Task.checkCancellation()
                        self?.negotiated.send(true)
                    case .answer:
                        logger.error("Unexpected answer received by acceptor")
                    case let .candidates(candidates):
                        await self?.handleRemoteCandidates(candidates, on: connectionWrapper)
                    case .closed:
                        await self?.clearPendingRemoteCandidates()
                        logger.debug("Remote closed signal received; stopping offer processing")
                        return
                    }
                }
            } catch is CancellationError {
                return
            } catch {
                self?.negotiated.send(false)
                logger.error("Error signal received: \(error)")
            }
        }
    }
}

extension CallAcceptor: CallCreatorProtocol {
    func setup() {
        logger.debug("Setup initiated")

        processLocalCandidates(from: connectionWrapper)
        processCallOffer()
    }
}
