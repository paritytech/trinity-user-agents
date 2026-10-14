import Foundation
import WebRTC

final class CallInitiator: CallCreator {
    private var initiationTask: Task<Void, Never>?
    private var processingTask: Task<Void, Never>?

    override func stopNegotiation() {
        super.stopNegotiation()

        initiationTask?.cancel()
        initiationTask = nil

        processingTask?.cancel()
        processingTask = nil
    }
}

private extension CallInitiator {
    func initiateCall() {
        initiationTask = Task { [weak self, connectionWrapper, signaling, logger] in
            do {
                await self?.applyLocalTracks()
                try Task.checkCancellation()

                let constrains = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
                let offer = try await connectionWrapper.offer(for: constrains)
                try Task.checkCancellation()

                logger.debug("Offer generated: \(offer.sdp.count)")
                try await connectionWrapper.setLocalDescription(offer)
                try Task.checkCancellation()

                logger.debug("Offer set as local description")
                let sendResult = try await signaling.send([.offer(offer.sdp)])

                guard sendResult.isFullySent else {
                    throw CallCreatorError.negotiationFailed
                }

                logger.debug("Offer sent to peer")
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled else { return }
                self?.negotiated.send(false)
                logger.error("Initiation failed: \(error)")
            }
        }
    }

    private func processCallAnswer() {
        processingTask = Task { [weak self, connectionWrapper, signaling, logger] in
            let incomingSignals = await (signaling.signals).eraseToAnyAsyncSequence()

            do {
                for try await signal in incomingSignals {
                    try Task.checkCancellation()

                    switch signal {
                    case .offer:
                        logger.error("Unexpected offer received by initiator")
                    case let .answer(sdp):
                        let currentState = await connectionWrapper.currentSignalingState()

                        guard currentState == .haveLocalOffer else {
                            logger.warning("Ignoring answer in \(currentState) state")
                            continue
                        }

                        logger.debug("Received answer: \(sdp.count)")
                        let remoteSdp = RTCSessionDescription(type: .answer, sdp: sdp)
                        try await connectionWrapper.setRemoteDescription(remoteSdp)
                        await self?.drainPendingRemoteCandidates(on: connectionWrapper)
                        logger.debug("Set remote answer")
                        self?.negotiated.send(true)
                    case let .candidates(candidates):
                        await self?.handleRemoteCandidates(
                            candidates,
                            on: connectionWrapper
                        )
                    case .closed:
                        await self?.clearPendingRemoteCandidates()
                        logger.debug("Remote closed signal received")
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

extension CallInitiator: CallCreatorProtocol {
    func setup() {
        processLocalCandidates(from: connectionWrapper)
        initiateCall()
        processCallAnswer()
    }
}
