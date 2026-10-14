import Foundation
import WebRTC
import AsyncExtensions
import AsyncAlgorithms

enum DataConnectionInitiatorError: Error {
    case dataChannelCreationFailed
}

final class DataConnectionInitiator: DataConnectionCreator {
    private let initiatorContext = Context()

    let purpose: String

    init(
        signaling: PeerConnectionSignaling,
        peerConnectionFactory: RTCPeerConnectionFactory,
        configFactory: WebRTCConfigMaking,
        purpose: String,
        candidateFilter: ConnectionCandidateFiltering? = nil,
        logger: LoggerProtocol
    ) {
        self.purpose = purpose

        super.init(
            signaling: signaling,
            peerConnectionFactory: peerConnectionFactory,
            configFactory: configFactory,
            candidateFilter: candidateFilter,
            logger: logger
        )
    }

    override func stopNegotiation() async {
        await super.stopNegotiation()
        await initiatorContext.stopNegotiation()
    }

    override func closeResources() async {
        await initiatorContext.closeResources()
        await super.closeResources()
    }
}

private extension DataConnectionInitiator {
    func setupDataChannel(
        on wrapper: AsyncPeerConnectionWrapper,
        purpose: String
    ) async throws -> AsyncDataChannelWrapper {
        let dataChannel: AsyncDataChannelWrapper
        do {
            dataChannel = try await wrapper.createDataChannel(
                label: purpose,
                configuration: configFactory.makeDataChannelConfiguration()
            )
        } catch {
            throw DataConnectionInitiatorError.dataChannelCreationFailed
        }

        guard await initiatorContext.setDataChannelWrapper(dataChannel) else {
            throw CancellationError()
        }

        return dataChannel
    }

    func processSignaling(on wrapper: AsyncPeerConnectionWrapper) async {
        let task = Task { [weak self, context, logger] in
            let incomingSignals = await context.signals

            do {
                for try await signal in incomingSignals {
                    guard !Task.isCancelled else {
                        return
                    }

                    do {
                        let shouldContinue = try await self?.processIncomingSignal(
                            signal,
                            on: wrapper
                        ) ?? false

                        guard shouldContinue else {
                            return
                        }
                    } catch is CancellationError {
                        return
                    } catch {
                        logger.error("Signal processing failed: \(error)")
                        await self?.cancel()
                        return
                    }
                }

                logger.debug("Signaling task finished")
            } catch {
                guard !Task.isCancelled else {
                    return
                }

                logger.error("Signal task failed: \(error)")
            }
        }

        await initiatorContext.setSignalingTask(task)
    }

    func processIncomingSignal(
        _ signal: PeerConnectionSignal,
        on wrapper: AsyncPeerConnectionWrapper
    ) async throws -> Bool {
        switch signal {
        case .offer:
            logger.error("Unexpected offer received by initiator")
        case let .answer(sdp):
            let currentState = await wrapper.currentSignalingState()
            guard currentState == .haveLocalOffer else {
                logger.warning(
                    "Ignoring answer in \(currentState) state"
                )
                return true
            }

            logger.debug("Received answer: \(sdp.count)")

            let remoteSdp = RTCSessionDescription(type: .answer, sdp: sdp)
            try await wrapper.setRemoteDescription(remoteSdp)
            await drainPendingRemoteCandidates(on: wrapper)

            logger.debug("Set remote answer")

            // once answer received auto send new candidates
            await context.startAutoflush()
        case let .candidates(sdpList):
            await handleRemoteCandidates(sdpList, on: wrapper)
        case .closed:
            await clearPendingRemoteCandidates()
            logger.debug("Remote closed signal received")
            return false
        }

        return true
    }

    func initiateChannelEstablishment(using wrapper: AsyncPeerConnectionWrapper) async {
        let task = Task { [weak self, context, logger] in
            do {
                // Create offer with only data channel (no media tracks yet)
                // This keeps the initial SDP small
                let constraints = RTCMediaConstraints(
                    mandatoryConstraints: nil,
                    optionalConstraints: nil
                )

                try Task.checkCancellation()

                let offer = try await wrapper.offer(for: constraints)
                try Task.checkCancellation()

                try await wrapper.setLocalDescription(offer)
                try Task.checkCancellation()

                logger.debug("Set local offer")

                // send offer and candidates discovered fast enough
                // slow candidates will be sent once we got an answer
                await context.sendSignalAndFlushBuffer(.offer(offer.sdp))
                try Task.checkCancellation()

                logger.debug("Sent offer: \(offer.sdp.count)")
            } catch is CancellationError {
                return
            } catch {
                logger.error("Data channel establishment failed: \(error)")
                await self?.cancel()
            }
        }

        await initiatorContext.setInitTask(task)
    }

    func createStateSequence(
        for connectionWrapper: AsyncPeerConnectionWrapper,
        dataWrapper: AsyncDataChannelWrapper
    ) -> AnyAsyncSequence<PeerDataConnectionState> {
        let signalingState = connectionWrapper.signalingState.eraseToAnyAsyncSequence()
        let iceState = connectionWrapper.iceConnectionState.eraseToAnyAsyncSequence()
        let dataChannelState = dataWrapper.state.eraseToAnyAsyncSequence()

        return combineLatest(
            combineLatest(signalingState, iceState),
            dataChannelState
        )
        .map { [logger] connectionStates, dataChannelState in
            let (optSignalingState, optIceState) = connectionStates

            logger.debug("Signaling: \(String(describing: optSignalingState))")
            logger.debug("ICE: \(String(describing: optIceState))")
            logger.debug("Data channel state: \(dataChannelState)")

            if let optIceState, optIceState.isTerminal {
                return .disconnected
            }

            switch (optSignalingState, dataChannelState) {
            case (.some(.stable), .open):
                let model = PeerDataConnectionState.Connected(
                    connection: connectionWrapper,
                    dataChannel: dataWrapper
                )

                return .connected(model)
            case (.some(.closed), _):
                return .disconnected
            case (_, .closed):
                return .disconnected
            case (.some(.haveRemoteOffer), _):
                return .connecting
            case (.some(.haveRemotePrAnswer), _):
                return .connecting
            case (.some(.stable), _):
                return .connecting
            default:
                return .waiting
            }
        }
        .eraseToAnyAsyncSequence()
    }
}

extension DataConnectionInitiator: DataConnectionCreating {
    func connect() async throws -> AnyAsyncSequence<PeerDataConnectionState> {
        let connectionWrapper = try await setupConnection()
        let dataChannelWrapper = try await setupDataChannel(
            on: connectionWrapper,
            purpose: purpose
        )

        await processSignaling(on: connectionWrapper)
        await processLocalCandidates(from: connectionWrapper)
        await initiateChannelEstablishment(using: connectionWrapper)

        return createStateSequence(for: connectionWrapper, dataWrapper: dataChannelWrapper)
    }
}

private extension DataConnectionInitiator {
    actor Context {
        private var dataChannelWrapper: AsyncDataChannelWrapper?
        private var signalingTask: Task<Void, Never>?
        private var initTask: Task<Void, Never>?
        private var isNegotiationStopped = false
        private var isClosingResources = false

        deinit {
            initTask?.cancel()
            signalingTask?.cancel()
        }

        func setDataChannelWrapper(_ wrapper: AsyncDataChannelWrapper) async -> Bool {
            guard !isClosingResources else {
                await wrapper.close()
                return false
            }

            dataChannelWrapper = wrapper

            return true
        }

        func setSignalingTask(_ task: Task<Void, Never>) {
            guard !isNegotiationStopped, !isClosingResources else {
                task.cancel()
                return
            }

            signalingTask?.cancel()
            signalingTask = task
        }

        func setInitTask(_ task: Task<Void, Never>) {
            guard !isNegotiationStopped, !isClosingResources else {
                task.cancel()
                return
            }

            initTask?.cancel()
            initTask = task
        }

        func stopNegotiation() {
            isNegotiationStopped = true

            initTask?.cancel()
            initTask = nil

            signalingTask?.cancel()
            signalingTask = nil
        }

        func closeResources() async {
            isClosingResources = true
            stopNegotiation()

            let wrapper = dataChannelWrapper
            dataChannelWrapper = nil

            await wrapper?.close()
        }
    }
}
