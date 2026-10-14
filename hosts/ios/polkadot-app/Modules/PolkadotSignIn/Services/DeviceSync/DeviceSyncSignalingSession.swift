import Foundation
import SubstrateSdk
import AsyncExtensions

/// WebRTC signaling over the device sync message transport (statement store).
///
/// `SyncSignalingEnvelope { offerId, message }` through the encrypted device
/// sync transport. SDP is encoded using MinimalSetup SCALE format (via
/// `SdpCoder`). ICE candidates are encoded using MinimalCandidate SCALE format.
actor DeviceSyncSignalingSession {
    private static let signalReplayBufferSize: UInt = 100

    private let transport: DeviceSyncMessageTransporting
    private let role: CallRole
    private let peerLogger: LoggerProtocol
    private let sdpCoder: SdpCoding = SdpCoder()
    private let signalBatcher: PeerConnectionSignalBatching
    private let offerIdHandler: @Sendable (String) async throws -> Void

    private var subject = AsyncReplaySubject<PeerConnectionSignal>(bufferSize: signalReplayBufferSize)
    private var activeOfferId: String?
    private var reconnectOfferId: String?
    private var pendingNegotiation: [Chat.DeviceSyncSignalingEnvelope] = []

    init(
        transport: DeviceSyncMessageTransporting,
        role: CallRole,
        signalBatcher: PeerConnectionSignalBatching = PeerConnectionSignalBatcher(),
        offerIdHandler: @escaping @Sendable (String) async throws -> Void = { _ in },
        peerLogger: LoggerProtocol
    ) {
        self.transport = transport
        self.role = role
        self.signalBatcher = signalBatcher
        self.offerIdHandler = offerIdHandler
        self.peerLogger = peerLogger
    }

    deinit {
        peerLogger.debug("Deinit")
    }
}

// MARK: - PeerConnectionSignaling

extension DeviceSyncSignalingSession: PeerConnectionSignaling {
    var signals: AnyAsyncSequence<PeerConnectionSignal> {
        get async { subject.eraseToAnyAsyncSequence() }
    }

    @discardableResult
    func send(
        _ signals: [PeerConnectionSignal]
    ) async throws -> PeerConnectionSignalSendResult {
        let batches = signalBatcher.batch(signals)
        let messages = batches.compactMap { encodeSignal($0) }
        await transport.send(messages)
        return batches.count == messages.count ? .fullySent : .notFullySent
    }
}

// MARK: - Incoming

extension DeviceSyncSignalingSession {
    func close() async {
        await transport.close()
    }

    func reconnection(
        in envelopes: [Chat.DeviceSyncSignalingEnvelope]
    ) -> (offerId: String, following: [Chat.DeviceSyncSignalingEnvelope])? {
        guard let reconnectOfferId else { return nil }
        guard let index = envelopes.lastIndex(where: {
            if case .reconnected = $0.message { return $0.offerId == reconnectOfferId }
            return false
        }) else { return nil }

        return (envelopes[index].offerId, Array(envelopes[(index + 1)...]))
    }

    func resetForReconnection() async {
        activeOfferId = nil
        subject = AsyncReplaySubject<PeerConnectionSignal>(bufferSize: Self.signalReplayBufferSize)
        if !pendingNegotiation.isEmpty {
            let negotiation = pendingNegotiation
            pendingNegotiation.removeAll()
            await handleIncomingEnvelopes(negotiation)
        }
    }

    func currentOfferId() -> String? { activeOfferId }
    func currentReconnectOfferId() -> String? { reconnectOfferId }

    func handleIncomingEnvelopes(_ envelopes: [Chat.DeviceSyncSignalingEnvelope]) async {
        guard !envelopes.isEmpty else { return }

        var latestOfferIndex: Int?
        for (index, envelope) in envelopes.enumerated() where envelope.message.isOffer {
            latestOfferIndex = index
        }

        for (index, envelope) in envelopes.enumerated() {
            if envelope.message.isOffer, index != latestOfferIndex {
                peerLogger.debug("Dropping stale offer in sync signaling batch")
                continue
            }

            await handleIncoming(envelope)
        }
    }
}

private extension DeviceSyncSignalingSession {
    func handleIncoming(_ envelope: Chat.DeviceSyncSignalingEnvelope) async {
        do {
            switch envelope.message {
            case .reconnected:
                break

            case let .offer(sdpData):
                try await handleOffer(sdpData, offerId: envelope.offerId)

            case let .answer(sdpData):
                try await handleAnswer(sdpData, offerId: envelope.offerId)

            case let .candidates(candidatesData):
                try handleCandidates(candidatesData, offerId: envelope.offerId)
            }
        } catch {
            peerLogger.error("Receive failed: \(error)")
        }
    }

    func handleOffer(_ sdpData: Data, offerId: String) async throws {
        guard role == .acceptor else {
            peerLogger.debug("Ignoring offer received by initiator")
            return
        }

        if let activeOfferId {
            if activeOfferId == offerId {
                peerLogger.debug("Dropping duplicate offer for active offerId=\(offerId)")
                return
            } else {
                pendingNegotiation = [
                    Chat.DeviceSyncSignalingEnvelope(
                        offerId: offerId,
                        message: .offer(sdpData)
                    )
                ]
                peerLogger.debug("Buffering offerId=\(offerId) while active=\(activeOfferId)")
                return
            }
        }

        peerLogger.debug("Offer (\(sdpData.count) bytes), offerId=\(offerId)")
        let decodedSetup = try sdpCoder.decodeSetup(sdpData)

        activeOfferId = offerId
        do {
            try await offerIdHandler(offerId)
        } catch {
            if activeOfferId == offerId {
                activeOfferId = nil
            }
            throw error
        }
        reconnectOfferId = offerId
        subject.send(.offer(decodedSetup.setupSdp))

        if !decodedSetup.candidates.isEmpty {
            peerLogger.debug("Offer included \(decodedSetup.candidates.count) candidate(s)")
            subject.send(.candidates(decodedSetup.candidates))
        }
    }

    func handleAnswer(_ sdpData: Data, offerId: String) async throws {
        guard role == .initiator else {
            peerLogger.debug("Ignoring answer received by acceptor")
            return
        }

        guard offerId == activeOfferId else {
            peerLogger.debug("Ignoring answer with mismatched offerId=\(offerId)")
            return
        }

        peerLogger.debug("Received answer (\(sdpData.count) bytes), offerId=\(offerId)")
        let decodedSetup = try sdpCoder.decodeSetup(sdpData)

        try await offerIdHandler(offerId)
        reconnectOfferId = offerId
        subject.send(.answer(decodedSetup.setupSdp))

        if !decodedSetup.candidates.isEmpty {
            peerLogger.debug("Answer included \(decodedSetup.candidates.count) candidate(s)")
            subject.send(.candidates(decodedSetup.candidates))
        }
    }

    func handleCandidates(_ data: Data, offerId: String) throws {
        if pendingNegotiation.first?.offerId == offerId {
            pendingNegotiation.append(
                Chat.DeviceSyncSignalingEnvelope(
                    offerId: offerId,
                    message: .candidates(data)
                )
            )
            peerLogger.debug("Buffering candidates for pending offerId=\(offerId)")
            return
        }

        guard offerId == activeOfferId else {
            peerLogger.debug("Ignoring candidates with mismatched offerId=\(offerId)")
            return
        }

        let candidates = try sdpCoder.decodeCandidates(data)
        peerLogger.debug("Received \(candidates.count) candidate(s), offerId=\(offerId)")
        subject.send(.candidates(candidates))
    }
}

// MARK: - Outgoing

private extension DeviceSyncSignalingSession {
    func encodeSignal(_ signal: BatchedPeerConnectionSignal) -> Data? {
        do {
            guard let envelope = try signalingEnvelope(from: signal) else {
                return nil
            }

            let encoder = ScaleEncoder()
            try envelope.encode(scaleEncoder: encoder)

            return encoder.encode()
        } catch {
            peerLogger.error("Sync signaling encode failed: \(error)")
            return nil
        }
    }

    func signalingEnvelope(
        from signal: BatchedPeerConnectionSignal
    ) throws -> Chat.DeviceSyncSignalingEnvelope? {
        switch signal {
        case let .offer(setup):
            let offerId = UUID().uuidString
            let envelope = try Chat.DeviceSyncSignalingEnvelope(
                offerId: offerId,
                message: .offer(sdpCoder.encodeSetup(setup))
            )
            activeOfferId = offerId
            reconnectOfferId = offerId
            peerLogger.debug("Sending offer: \(setup.setupSdp.count) chars, offerId=\(offerId)")
            return envelope

        case let .answer(setup):
            guard let offerId = activeOfferId else {
                peerLogger.error("Cannot send answer without active offerId")
                return nil
            }
            peerLogger.debug("Sending answer: \(setup.setupSdp.count) chars, offerId=\(offerId)")
            return try .init(offerId: offerId, message: .answer(sdpCoder.encodeSetup(setup)))

        case let .candidates(candidates):
            guard let offerId = activeOfferId else {
                peerLogger.error("Cannot send candidates without active offerId")
                return nil
            }
            peerLogger.debug("Sending candidates: \(candidates.count) candidates, offerId=\(offerId)")
            return try .init(offerId: offerId, message: .candidates(sdpCoder.encodeCandidates(candidates)))

        case .closed:
            peerLogger.debug("Sync close signal - no-op over device session")
            return nil
        }
    }
}

extension DeviceSyncSignalingSession {
    func sendReconnected(offerId: String) async {
        do {
            let envelope = Chat.DeviceSyncSignalingEnvelope(
                offerId: offerId,
                message: .reconnected
            )
            let encoder = ScaleEncoder()
            try envelope.encode(scaleEncoder: encoder)
            await transport.send([encoder.encode()])
            peerLogger.debug("Sent Reconnected signal for offerId=\(offerId)")
        } catch {
            peerLogger.error("Failed to send Reconnected: \(error)")
        }
    }
}
