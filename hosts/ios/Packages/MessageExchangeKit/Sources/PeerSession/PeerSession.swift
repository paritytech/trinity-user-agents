import Foundation
import Foundation_iOS
import SubstrateSdk
import StatementStore
import SDKLogger

final class PeerSession<M: MessageExchange.CodableMessage>: TypeErasedDelegateStoring {
    typealias Message = M

    let peer: MessageExchange.Peer
    let sessionId: MessageExchange.SessionId

    weak var delegate: AnyPeerSessionDelegate<M>?

    private let workQueue: DispatchQueue
    private let outgoingChannel: AnyOutgoingMessageChannel<M>
    private let incomingChannel: AnyIncomingMessageChannel<M>
    private let peerSubscription: any PeerSessionStatementSubscribing
    private let initializer: PeerSessionInitializing
    private let priorityProvider: PeerSessionPriorityProviding
    private let statementDataCoder: StatementDataCoding
    private let routeContexts: PeerSessionRoute.Contexts
    private let logger: SDKLoggerProtocol?

    private var state = InternalSessionState<M>.idle {
        didSet { didSetState() }
    }

    init(
        workQueue: DispatchQueue,
        peer: MessageExchange.Peer,
        sessionId: MessageExchange.SessionId,
        outgoingChannel: AnyOutgoingMessageChannel<M>,
        incomingChannel: AnyIncomingMessageChannel<M>,
        peerSubscription: any PeerSessionStatementSubscribing,
        initializer: PeerSessionInitializing,
        priorityProvider: PeerSessionPriorityProviding,
        statementDataCoder: StatementDataCoding,
        routeContexts: PeerSessionRoute.Contexts,
        logger: SDKLoggerProtocol?
    ) {
        self.workQueue = workQueue
        self.peer = peer
        self.sessionId = sessionId
        self.outgoingChannel = outgoingChannel
        self.incomingChannel = incomingChannel
        self.peerSubscription = peerSubscription
        self.initializer = initializer
        self.priorityProvider = priorityProvider
        self.statementDataCoder = statementDataCoder
        self.routeContexts = routeContexts
        self.logger = logger

        initializeSession()
    }
}

extension PeerSession: PeerSessionProtocol {
    func addMessagesToQueue(_ messages: [Message]) {
        workQueue.async { [weak self] in
            self?.outgoingChannel.addMessagesToQueue(messages)
        }
    }
}

extension PeerSession: PeerSessionInitializerDelegate {
    func sessionInitializer(
        _: any PeerSessionInitializing,
        didInitializeWith result: SessionInitializationSuccess<M>
    ) {
        guard delegate != nil else {
            logger?.debug("Delegate is nil, skipping initialization")
            return
        }

        priorityProvider.expiry = result.priority
        state = .active(restoredRequests: result.outgoingState.outgoingRequests)

        delegate?.peerSession(
            self,
            didInitializeWithOutgoingMessages: result.outgoingState.initializedMessages
        )

        // TODO: Response statements share the same response channel, so replying to
        // multiple restored requests can leave only the latest ACK visible to an
        // offline sender. Message delivery is still done per request here; revisit
        // the protocol if sender-side ACK recovery must cover every request id.
        result.incomingState.peerRequests.forEach { peerRequest in
            _ = handlePeerRequest(peerRequest.request, route: peerRequest.route)
        }

        for (route, peerResponse) in result.outgoingState.peerResponses {
            _ = handlePeerResponse(peerResponse, route: route)
        }

        updatePolling()
    }

    func sessionInitializer(
        _ initializer: any PeerSessionInitializing,
        didFailToInitializeWith result: SessionInitializationFailure
    ) {
        guard delegate != nil else {
            logger?.debug("Delegate is nil, skipping initialization failure handling")
            return
        }

        let shouldReset = delegate?.peerSession(
            self,
            shouldResetAfter: result.error
        ) ?? MessageExchange.shouldResetSession

        guard shouldReset else {
            state = .idle
            updatePolling()
            return
        }

        sessionInitializer(
            initializer,
            didInitializeWith: .init(priority: result.expiry)
        )
    }
}

extension PeerSession: IncomingMessageChannelDelegate, OutgoingMessageChannelDelegate {
    func messageChannel(
        _: any OutgoingMessageChanneling,
        didFinishAddingMessageToQueue message: Message,
        withError error: MessageExchange.AddToQueueError?
    ) {
        delegate?.peerSession(
            self,
            didFinishAddingMessageToQueue: message,
            withError: error
        )
    }

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didPostMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        delegate?.peerSession(
            self,
            didPostMessages: messages,
            withError: error
        )
    }

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didDeliverMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        delegate?.peerSession(
            self,
            didDeliverMessages: messages,
            withError: error
        )
    }

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didCompactMessages compactedMessage: Message,
        originalMessages: [Message]
    ) {
        delegate?.peerSession(
            self,
            didCompactMessages: compactedMessage,
            originalMessages: originalMessages
        )
    }

    func statementSubmitFailed(with error: Error) {
        if isStatementErrorRequiresReinitialization(error) {
            initializeSession()
            return
        }

        let shouldReinit = delegate?.peerSession(
            self,
            shouldReinitializeAfterSubmitError: error
        ) ?? MessageExchange.shouldReinitializeSession

        if shouldReinit {
            initializeSession()
        }
    }
}

private extension PeerSession {
    func didSetState() {
        logger?.debug("State updated to \(state)")
        delegate?.peerSession(self, didUpdateState: state.toPeerSessionState())

        switch state {
        case let .active(restoredRequests):
            outgoingChannel.activate(restoringState: restoredRequests)
        case .idle,
             .initializing:
            outgoingChannel.deactivate()
        }
    }

    func updatePolling() {
        if state.isActive {
            peerSubscription.start { [weak self] sessionStatement in
                self?.handlePollingStatement(sessionStatement) ?? false
            }
        } else {
            peerSubscription.stop()
        }
    }

    func initializeSession() {
        workQueue.async { [weak self] in
            self?.performInitializeSession()
        }
    }

    func performInitializeSession() {
        guard !state.isInitializing else {
            logger?.debug("Already initializing")
            return
        }

        state = .initializing
        updatePolling()

        initializer.initializeSession()
    }

    func isStatementErrorRequiresReinitialization(_ error: Error) -> Bool {
        guard let submittionError = error as? StatementSubmitError else {
            return false
        }

        switch submittionError {
        case let .rejected(reason):
            // channelPriorityTooLow is delegated to the app: a routine burst of
            // outgoing requests can outbid each other on expiry, and that does
            // not warrant tearing down the session.
            return reason != .channelPriorityTooLow
        case .invalid,
             .internalError:
            return true
        case .unexpectedStatus:
            return false
        }
    }

    func handlePollingStatement(_ sessionStatement: PeerSessionStatement) -> StatementHandlingStatus {
        guard delegate != nil else {
            logger?.debug("Delegate is nil, skipping statement")
            return false
        }

        guard state.isActive else {
            logger?.debug("Session is not active")
            return false
        }

        let statement = sessionStatement.statement

        guard let encodedDataPayload = statement.getScaleEncodedPayload() else {
            return handleIncomingMessageError(.decodingFailed, for: sessionStatement)
        }

        let senderAccountId = statement.getSenderAccountId()

        do {
            let outcome: StatementDataDecodingResult<Message> = try statementDataCoder
                .decodeFromScaleEncodedPayload(
                    encodedDataPayload,
                    senderAccountId: senderAccountId,
                    route: sessionStatement.route
                )

            switch outcome {
            case let .statementData(statementData):
                return handleStatementData(statementData, route: sessionStatement.route)
            case let .requestId(requestId, error):
                return handleFailedPeerRequest(requestId, error: error, route: sessionStatement.route)
            }
        } catch {
            return handleIncomingMessageError(makeIncomingMessageError(error: error), for: sessionStatement)
        }
    }

    func handleStatementData(
        _ statementData: StatementData<Message>,
        route: PeerSessionRoute
    ) -> StatementHandlingStatus {
        switch statementData {
        case let .request(request):
            return handlePeerRequest(request, route: route)
        case let .response(response):
            return handlePeerResponse(response, route: route)
        case .multirequest,
             .multiresponse:
            // Peer sent a multi-device envelope but this session uses a legacy coder
            // that cannot decrypt the inner payload. This happens when the peer's
            // device_added message hasn't been received yet.
            logger?.warning("Skipping multi-device envelope: peer devices not yet known")
            return false
        }
    }

    func handlePeerRequest(
        _ request: MessageExchange.Request<Message>,
        route: PeerSessionRoute
    ) -> StatementHandlingStatus {
        let requestId = request.requestId

        logger?.debug(
            "Successfully received request \(requestId) with \(request.messages.count) messages on route \(route)"
        )

        delegate?.peerSession(
            self,
            didReceiveMessages: request.messages
        ) { [weak self] responseCode in
            self?.workQueue.async { [weak self] in
                self?.incomingChannel.sendResponse(
                    with: responseCode,
                    forRequestId: requestId,
                    route: route
                )
            }
        }

        return true
    }

    func handleFailedPeerRequest(
        _ requestId: String,
        error: Error,
        route: PeerSessionRoute
    ) -> StatementHandlingStatus {
        logger?.debug("Received failed decoding request \(requestId) \(error)")

        delegate?.peerSessionDidReceiveMessagesError(self) { [weak self] responseCode in
            self?.workQueue.async { [weak self] in
                self?.incomingChannel.sendResponse(
                    with: responseCode,
                    forRequestId: requestId,
                    route: route
                )
            }
        }

        return true
    }

    func handlePeerResponse(
        _ response: MessageExchange.Response,
        route: PeerSessionRoute
    ) -> StatementHandlingStatus {
        if outgoingChannel.handleResponse(response, route: route) {
            initializer.setLastHandledResponseId(response.requestId, route: route)
            return true
        } else {
            logger?.debug("Response \(response.requestId) on route \(route) was not handled by outgoing channel")
            return false
        }
    }

    func handleIncomingMessageError(
        _ error: MessageExchange.IncomingMessageError,
        for sessionStatement: PeerSessionStatement
    ) -> StatementHandlingStatus {
        let statement = sessionStatement.statement
        let shouldIgnore = delegate?.peerSession(
            self,
            shouldIgnoreStatementAfter: error
        ) ?? MessageExchange.shouldIgnoreStatement

        let peerRequestChannelId = routeContexts
            .context(for: sessionStatement.route)
            .peerRequestChannelId

        guard
            let channel = statement.getChannel(),
            let requestChannel = try? peerRequestChannelId.fixedStatementFieldData(),
            channel == requestChannel
        else {
            logger?.error("Failed to handle incoming message: \(error)")
            return shouldIgnore
        }

        logger?.error("Failed to handle incoming request: \(error)")

        return shouldIgnore
    }

    func makeIncomingMessageError(error: Error) -> MessageExchange.IncomingMessageError {
        switch error {
        case let decodingError as StatementDataDecodingError:
            return makeIncomingMessageError(decodingError: decodingError)
        case let multiDeviceError as MultiDeviceDecodingError:
            return makeIncomingMessageError(multiDeviceError: multiDeviceError)
        default:
            logger?.error("Unexpected error: \(error)")
            return .decodingFailed
        }
    }

    func makeIncomingMessageError(
        decodingError: StatementDataDecodingError
    ) -> MessageExchange.IncomingMessageError {
        switch decodingError {
        case .decodingFailed:
            .decodingFailed
        case .decryptionFailed:
            .decryptionFailed
        }
    }

    func makeIncomingMessageError(
        multiDeviceError: MultiDeviceDecodingError
    ) -> MessageExchange.IncomingMessageError {
        switch multiDeviceError {
        case .payloadDecodingFailed:
            .decodingFailed
        case .deviceEntryNotFound,
             .oneshotKeyDecryptionFailed,
             .payloadDecryptionFailed:
            .decryptionFailed
        }
    }
}

public enum PeerSessionState {
    case idle
    case initializing
    case active
}

enum InternalSessionState<M: MessageExchange.CodableMessage> {
    case idle
    case initializing
    case active(restoredRequests: [PeerSessionRoute: OutgoingRequest<M>])

    var isInitializing: Bool {
        switch self {
        case .idle,
             .active:
            false
        case .initializing:
            true
        }
    }

    var isActive: Bool {
        switch self {
        case .idle,
             .initializing:
            false
        case .active:
            true
        }
    }

    func toPeerSessionState() -> PeerSessionState {
        switch self {
        case .idle:
            .idle
        case .initializing:
            .initializing
        case .active:
            .active
        }
    }
}
