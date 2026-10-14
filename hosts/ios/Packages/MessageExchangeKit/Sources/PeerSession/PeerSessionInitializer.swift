import Foundation
import SubstrateSdk
import StatementStore
import SDKLogger

protocol PeerSessionInitializing {
    func initializeSession()
    func setLastHandledResponseId(_ responseId: String?, route: PeerSessionRoute)
}

struct SessionInitializationSuccess<Message: MessageExchange.CodableMessage> {
    let outgoingState: OutgoingInitializationState<Message>
    let incomingState: IncomingInitializationState<Message>
    let priority: UInt64

    init(
        outgoingState: OutgoingInitializationState<Message>,
        incomingState: IncomingInitializationState<Message>,
        priority: UInt64
    ) {
        self.outgoingState = outgoingState
        self.incomingState = incomingState
        self.priority = priority
    }

    init(priority: UInt64) {
        outgoingState = .empty
        incomingState = .empty

        self.priority = priority
    }
}

struct OutgoingInitializationState<Message: MessageExchange.CodableMessage> {
    let outgoingRequests: [PeerSessionRoute: OutgoingRequest<Message>]
    let peerResponses: [PeerSessionRoute: MessageExchange.Response]
    let initializedMessages: [Message]

    static var empty: Self {
        .init(
            outgoingRequests: [:],
            peerResponses: [:],
            initializedMessages: []
        )
    }
}

struct IncomingInitializationState<Message: MessageExchange.CodableMessage> {
    let peerRequests: [PeerSessionRequest<Message>]

    static var empty: Self {
        .init(
            peerRequests: []
        )
    }
}

struct SessionInitializationFailure {
    let error: MessageExchange.InitializationError
    let expiry: UInt64
}

final class PeerSessionInitializer<M: MessageExchange.CodableMessage> {
    typealias Message = M

    weak var delegate: AnyPeerSessionInitializerDelegate<M>?

    private let priorityProvider: PeerSessionPriorityProviding
    private let ownSubscription: any PeerSessionStatementSubscribing
    private let peerSubscription: any PeerSessionStatementSubscribing
    private let workQueue: DispatchQueue
    private let statementDataCoder: StatementDataCoding
    private let supportedRoutes: [PeerSessionRoute]
    private let logger: SDKLoggerProtocol?

    private var isInitializing = false
    private var lastHandledResponseIds = [PeerSessionRoute: String]()

    init(
        priorityProvider: PeerSessionPriorityProviding,
        ownSubscription: any PeerSessionStatementSubscribing,
        peerSubscription: any PeerSessionStatementSubscribing,
        workQueue: DispatchQueue,
        statementDataCoder: StatementDataCoding,
        supportedRoutes: [PeerSessionRoute],
        logger: SDKLoggerProtocol?
    ) {
        self.priorityProvider = priorityProvider
        self.ownSubscription = ownSubscription
        self.peerSubscription = peerSubscription
        self.workQueue = workQueue
        self.statementDataCoder = statementDataCoder
        self.supportedRoutes = supportedRoutes
        self.logger = logger
    }
}

extension PeerSessionInitializer: PeerSessionInitializing {
    func initializeSession() {
        workQueue.async { [weak self] in
            self?.performSessionInitialization()
        }
    }

    func setLastHandledResponseId(
        _ responseId: String?,
        route: PeerSessionRoute
    ) {
        lastHandledResponseIds[route] = responseId
    }
}

private extension PeerSessionInitializer {
    struct DecodedStatement {
        let statementData: StatementData<Message>
        let statement: Statement
        let route: PeerSessionRoute
    }

    func performSessionInitialization() {
        guard !isInitializing else {
            logger?.debug("Already initializing")
            return
        }

        isInitializing = true

        ownSubscription.resetSeenHashes()
        peerSubscription.resetSeenHashes()

        let group = DispatchGroup()

        var ownStatements = [PeerSessionStatement]()
        var ownError: StatementSubscriptionError?

        var peerStatements = [PeerSessionStatement]()
        var peerError: StatementSubscriptionError?

        group.enter()
        ownSubscription.fetchOnce(handler: {
            ownStatements.append($0)
            return true
        }, completion: { error in
            ownError = error
            group.leave()
        })

        group.enter()
        peerSubscription.fetchOnce(handler: {
            peerStatements.append($0)
            return true
        }, completion: { error in
            peerError = error
            group.leave()
        })

        group.notify(queue: workQueue) { [weak self] in
            self?.continueAfterFetch(
                ownStatements: ownStatements,
                peerStatements: peerStatements,
                ownError: ownError,
                peerError: peerError
            )
        }
    }

    func continueAfterFetch(
        ownStatements: [PeerSessionStatement],
        peerStatements: [PeerSessionStatement],
        ownError: StatementSubscriptionError?,
        peerError: StatementSubscriptionError?
    ) {
        logger?.debug("Own statements count \(ownStatements.count)")
        logger?.debug("Peer statements count \(peerStatements.count)")
        if let ownError { logger?.error("Own error: \(ownError)") }
        if let peerError { logger?.error("Peer error: \(peerError)") }

        let expiry = priorityProvider.initialExpiry(from: ownStatements.map(\.statement))

        logger?.debug("Resolved priority: \(expiry)")

        do {
            if let pollingError = ownError ?? peerError {
                throw pollingError
            }

            let sortedOwnStatements = ownStatements
                .sorted { ($0.statement.getExpiry() ?? 0) > ($1.statement.getExpiry() ?? 0) }
            let sortedPeerStatements = peerStatements
                .sorted { ($0.statement.getExpiry() ?? 0) > ($1.statement.getExpiry() ?? 0) }

            let ownStatementDataList = try makeStatementDataList(from: sortedOwnStatements)
            let peerStatementDataList = try makeStatementDataList(from: sortedPeerStatements)

            let incomingState = try makeIncomingState(
                ownStatementDataList: ownStatementDataList,
                peerStatementDataList: peerStatementDataList
            )

            let outgoingState = try makeOutgoingState(
                ownStatementDataList: ownStatementDataList,
                peerStatementDataList: peerStatementDataList
            )

            isInitializing = false

            delegate?.sessionInitializer(
                self,
                didInitializeWith: .init(
                    outgoingState: outgoingState,
                    incomingState: incomingState,
                    priority: expiry
                )
            )
        } catch {
            isInitializing = false

            let initError = makeInitializationError(error: error)
            logger?.error("Session initialization failed: \(initError) (underlying: \(error))")

            delegate?.sessionInitializer(
                self,
                didFailToInitializeWith: .init(
                    error: initError,
                    expiry: expiry
                )
            )
        }
    }

    func makeStatementDataList(
        from statements: [PeerSessionStatement]
    ) throws -> [DecodedStatement] {
        try statements.compactMap { sessionStatement -> DecodedStatement? in
            let statement = sessionStatement.statement

            guard let payload = statement.getScaleEncodedPayload() else {
                return nil
            }

            let senderAccountId = statement.getSenderAccountId()
            let result: StatementDataDecodingResult<Message>

            do {
                result = try statementDataCoder.decodeFromScaleEncodedPayload(
                    payload,
                    senderAccountId: senderAccountId,
                    route: sessionStatement.route
                )
            } catch {
                if shouldSkipInitializerDecodeError(error, route: sessionStatement.route) {
                    return nil
                }
                throw error
            }

            switch result {
            case let .statementData(statementData):
                return DecodedStatement(
                    statementData: statementData,
                    statement: statement,
                    route: sessionStatement.route
                )
            case let .requestId(requestId, _):
                // it is ok to return statement data for compatibility reason here
                // since we don't respond to requests in the initializer
                return DecodedStatement(
                    statementData: StatementData.request(
                        .init(requestId: requestId, messages: [])
                    ),
                    statement: statement,
                    route: sessionStatement.route
                )
            }
        }
    }

    func shouldSkipInitializerDecodeError(
        _ error: Error,
        route: PeerSessionRoute
    ) -> Bool {
        guard route == .device else {
            return false
        }

        guard let decodingError = error as? MultiDeviceDecodingError else {
            return false
        }

        switch decodingError {
        case .deviceEntryNotFound,
             .oneshotKeyDecryptionFailed,
             .payloadDecryptionFailed:
            return true
        case .payloadDecodingFailed:
            return false
        }
    }

    func makeIncomingState(
        ownStatementDataList: [DecodedStatement],
        peerStatementDataList: [DecodedStatement]
    ) throws -> IncomingInitializationState<Message> {
        let pendingPeerRequests = peerStatementDataList.compactMap { decodedStatement
            -> PeerSessionRequest<Message>? in
            guard case let .request(peerRequest) = decodedStatement.statementData else {
                return nil
            }

            // TODO: Response statements share the same response channel, so init
            // can only match request ids whose ACK is still visible in fetched peer
            // statements. Revisit this if sender-side ACK recovery must cover every
            // outstanding request id.
            let responseStatementData = matchingResponseStatementData(
                forRequestStatementData: decodedStatement,
                in: ownStatementDataList
            )

            if responseStatementData == nil {
                return .init(request: peerRequest, route: decodedStatement.route)
            } else {
                return nil
            }
        }

        guard !pendingPeerRequests.isEmpty else {
            return .empty
        }

        return .init(peerRequests: pendingPeerRequests)
    }

    func makeOutgoingState(
        ownStatementDataList: [DecodedStatement],
        peerStatementDataList: [DecodedStatement]
    ) throws -> OutgoingInitializationState<Message> {
        let outgoingRequestStatements = supportedRoutes.compactMap { route in
            ownStatementDataList.first(where: { decodedStatement in
                if decodedStatement.route == route,
                   case .request = decodedStatement.statementData {
                    true
                } else {
                    false
                }
            })
        }

        guard !outgoingRequestStatements.isEmpty else {
            return .empty
        }

        var outgoingRequests = [PeerSessionRoute: OutgoingRequest<Message>]()
        var peerResponses = [PeerSessionRoute: MessageExchange.Response]()
        var initializedMessages = [Message]()

        for outgoingRequestStatement in outgoingRequestStatements {
            guard
                case let .request(request) = outgoingRequestStatement.statementData,
                let scaleEncodedPayload = outgoingRequestStatement.statement.getScaleEncodedPayload()
            else {
                continue
            }

            let responseStatementData = matchingResponseStatementData(
                forRequestStatementData: outgoingRequestStatement,
                in: peerStatementDataList
            )

            if case let .response(response) = responseStatementData {
                if response.requestId != lastHandledResponseIds[outgoingRequestStatement.route] {
                    outgoingRequests[outgoingRequestStatement.route] = .init(
                        requestId: request.requestId,
                        messages: request.messages,
                        scaleEncodedPayload: scaleEncodedPayload,
                        route: outgoingRequestStatement.route
                    )
                    peerResponses[outgoingRequestStatement.route] = response
                }
            } else {
                outgoingRequests[outgoingRequestStatement.route] = .init(
                    requestId: request.requestId,
                    messages: request.messages,
                    scaleEncodedPayload: scaleEncodedPayload,
                    route: outgoingRequestStatement.route
                )
                initializedMessages += request.messages
            }
        }

        return .init(
            outgoingRequests: outgoingRequests,
            peerResponses: peerResponses,
            initializedMessages: initializedMessages
        )
    }

    func matchingResponseStatementData(
        forRequestStatementData requestStatementData: DecodedStatement,
        in statementDataList: [DecodedStatement]
    ) -> StatementData<Message>? {
        statementDataList.first(where: { decodedStatement in
            if decodedStatement.route == requestStatementData.route,
               case let .response(response) = decodedStatement.statementData,
               case let .request(request) = requestStatementData.statementData,
               response.requestId == request.requestId {
                true
            } else {
                false
            }
        })?.statementData
    }

    func makeInitializationError(error: Error) -> MessageExchange.InitializationError {
        switch error {
        case let initializationError as MessageExchange.InitializationError:
            initializationError
        case let pollingError as StatementSubscriptionError:
            makeInitializationError(pollingError: pollingError)
        case let incomingMessageError as MessageExchange.IncomingMessageError:
            makeInitializationError(incomingMessageError: incomingMessageError)
        case let decodingError as StatementDataDecodingError:
            makeInitializationError(decodingError: decodingError)
        default:
            .other(error)
        }
    }

    func makeInitializationError(pollingError: StatementSubscriptionError) -> MessageExchange.InitializationError {
        switch pollingError {
        case .statementDecodingFailed:
            .statementDecodingFailed
        case let .other(error):
            .other(error)
        }
    }

    func makeInitializationError(incomingMessageError: MessageExchange.IncomingMessageError) -> MessageExchange
        .InitializationError {
        switch incomingMessageError {
        case .decryptionFailed:
            .statementPayloadDecryptionFailed
        case .decodingFailed:
            .statementPayloadDecodingFailed
        }
    }

    func makeInitializationError(decodingError: StatementDataDecodingError) -> MessageExchange.InitializationError {
        switch decodingError {
        case .decodingFailed:
            .statementPayloadDecodingFailed
        case .decryptionFailed:
            .statementPayloadDecryptionFailed
        }
    }
}
