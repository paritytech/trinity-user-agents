import Foundation
import SubstrateSdk
import AsyncExtensions
import Operation_iOS

enum ChatPeerConnectionSignalerError: Error {
    case undefinedOfferId
    case offerIdMismatch(String?, String)
}

actor ChatPeerConnectionSignaler {
    nonisolated let peerAccountId: AccountId
    nonisolated let callType: ChatCallType
    nonisolated let messagesStorageService: MessagesLocalStorageServicing
    nonisolated let outboxService: ChatOutboxServicing
    nonisolated let providerFactory: ChatMessageDataProviderMaking
    nonisolated let workQueue: DispatchQueue
    nonisolated let logger: LoggerProtocol
    nonisolated let sdpCoder = SdpCoder()

    private nonisolated let subject = AsyncReplaySubject<PeerConnectionSignal>(bufferSize: 100)
    private nonisolated let messageEventsSubject = AsyncReplaySubject<CallMessageEvent>(bufferSize: 10)
    private nonisolated let signalBatcher: PeerConnectionSignalBatching

    private var offerId: String?
    private var processedMessageIds = Set<Chat.MessageId>()

    init(
        peerAccountId: AccountId,
        callType: ChatCallType,
        outboxService: ChatOutboxServicing,
        messagesStorageService: MessagesLocalStorageServicing = MessagesLocalStorageService(),
        providerFactory: ChatMessageDataProviderMaking = ChatMessageDataProviderFactory(),
        workQueue: DispatchQueue = DispatchQueue(
            label: "ChatPeerConnectionSignaler.queue",
            qos: .utility
        ),
        signalBatcher: PeerConnectionSignalBatching = PeerConnectionSignalBatcher(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.peerAccountId = peerAccountId
        self.callType = callType
        self.outboxService = outboxService
        self.messagesStorageService = messagesStorageService
        self.providerFactory = providerFactory
        self.workQueue = workQueue
        self.signalBatcher = signalBatcher
        self.logger = logger
    }

    deinit {
        logger.debug("Deinit")
    }
}

private extension ChatPeerConnectionSignaler {
    func sendBatch(_ batch: BatchedPeerConnectionSignal) async throws {
        switch batch {
        case let .offer(setup):
            let minimizedSdp = try sdpCoder.encodeSetup(setup)
            let content = Chat.RemoteMessageContentV1.MessageContent.DataChannelOfferContent(
                sdp: minimizedSdp,
                purpose: callType.toRemote()
            )

            logger.debug("Sent message with offer: \(minimizedSdp.count)")
            logger.debug("Setup candidates: \(setup.candidates.count)")

            let message = makeLocalCallMessage(.offer(content))
            try await persistCallMessage(message)
            offerId = message.messageId
        case let .answer(setup):
            guard let offerId else {
                throw ChatPeerConnectionSignalerError.undefinedOfferId
            }

            let minimizedSdp = try sdpCoder.encodeSetup(setup)

            logger.debug("Sent message with answer: \(minimizedSdp.count)")
            logger.debug("Setup candidates: \(setup.candidates.count)")

            let content = Chat.RemoteMessageContentV1.MessageContent.DataChannelAnswerContent(
                offerId: offerId,
                sdp: minimizedSdp
            )

            let message = makeLocalCallMessage(.answer(content))
            try await persistCallMessage(message)
        case let .candidates(candidates):
            guard let offerId else {
                throw ChatPeerConnectionSignalerError.undefinedOfferId
            }

            let minimizedSdp = try sdpCoder.encodeCandidates(candidates)

            let content = Chat.RemoteMessageContentV1.MessageContent.DataChannelCandidatesContent(
                offerId: offerId,
                sdp: minimizedSdp
            )

            logger.debug("Sent message with candidates: \(minimizedSdp.count)")

            let remoteMessage = Chat.RemoteMessage.newMessage(with: .dataChannelCandidates(content))
            outboxService.sendDirectly(remoteMessage, to: peerAccountId)
        case .closed:
            guard let offerId else {
                throw ChatPeerConnectionSignalerError.undefinedOfferId
            }

            let content = Chat.RemoteMessageContentV1.MessageContent.DataChannelClosedContent(
                offerId: offerId
            )

            logger.debug("Sent close message for offer: \(offerId)")

            let message = makeLocalCallMessage(.closed(content))
            try await persistCallMessage(message)
        }
    }

    func makeLocalCallMessage(
        _ payload: Chat.LocalMessage.Content.CallSignalingPayload
    ) -> Chat.LocalMessage {
        Chat.LocalMessage.newMessageToPerson(
            peerAccountId,
            content: .call(payload)
        )
    }

    func persistCallMessage(_ message: Chat.LocalMessage) async throws {
        let operation = messagesStorageService.insertOrUpdate([message])
        try await operation.asyncExecute()
    }
}

extension ChatPeerConnectionSignaler: PeerConnectionSignaling {
    var signals: AnyAsyncSequence<PeerConnectionSignal> {
        subject.eraseToAnyAsyncSequence()
    }

    @discardableResult
    func send(
        _ signals: [PeerConnectionSignal]
    ) async throws -> PeerConnectionSignalSendResult {
        var isFullySent = true

        for batch in signalBatcher.batch(signals) {
            do {
                try await sendBatch(batch)
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                logger.error("Signal batch send failed: \(error)")
                isFullySent = false
            }
        }

        return PeerConnectionSignalSendResult(isFullySent: isFullySent)
    }
}

extension ChatPeerConnectionSignaler: CallMessageEventProviding {
    nonisolated var messageEvents: AnyAsyncSequence<CallMessageEvent> {
        messageEventsSubject.eraseToAnyAsyncSequence()
    }
}

extension ChatPeerConnectionSignaler: ChatCallMessageHandling {
    func receive(message: Chat.RemoteMessage) async {
        guard !processedMessageIds.contains(message.messageId) else {
            logger.warning("Message already processed: \(message.messageId)")
            return
        }

        processedMessageIds.insert(message.messageId)

        do {
            switch message.versioned.ensureV1()?.content {
            case let .dataChannelOffer(content):
                let decodedSetup = try sdpCoder.decodeSetup(content.sdp)

                offerId = message.messageId
                subject.send(.offer(decodedSetup.setupSdp))

                if !decodedSetup.candidates.isEmpty {
                    subject.send(.candidates(decodedSetup.candidates))
                }

            case let .dataChannelAnswer(content):
                guard content.offerId == offerId else {
                    throw ChatPeerConnectionSignalerError.offerIdMismatch(
                        offerId,
                        content.offerId
                    )
                }

                let decodedSetup = try sdpCoder.decodeSetup(content.sdp)
                subject.send(.answer(decodedSetup.setupSdp))

                if !decodedSetup.candidates.isEmpty {
                    subject.send(.candidates(decodedSetup.candidates))
                }

            case let .dataChannelCandidates(content):
                guard content.offerId == offerId else {
                    throw ChatPeerConnectionSignalerError.offerIdMismatch(
                        offerId,
                        content.offerId
                    )
                }

                let decodedCandidates = try sdpCoder.decodeCandidates(content.sdp)

                subject.send(.candidates(decodedCandidates))

            case let .dataChannelClosed(content):
                guard content.offerId == offerId else {
                    throw ChatPeerConnectionSignalerError.offerIdMismatch(
                        offerId,
                        content.offerId
                    )
                }

                subject.send(.closed)

            default:
                break
            }
        } catch {
            logger.error("Sdp decoding failed: \(error)")
        }
    }

    func handleSent(message: Chat.RemoteMessage) async {
        guard case let .dataChannelClosed(content) = message.versioned.ensureV1()?.content else {
            return
        }

        guard content.offerId == offerId else {
            logger.warning("Ignoring sent close with unexpected offer id: \(content.offerId)")
            return
        }

        messageEventsSubject.send(.closedSent)
    }

    func handleDelivered(message: Chat.RemoteMessage) async {
        guard case .dataChannelOffer = message.versioned.ensureV1()?.content else {
            return
        }

        guard message.messageId == offerId else {
            logger.warning("Ignoring delivered offer with unexpected id: \(message.messageId)")
            return
        }

        messageEventsSubject.send(.offerDelivered)
    }
}
