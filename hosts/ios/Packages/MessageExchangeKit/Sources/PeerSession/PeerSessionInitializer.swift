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

        let sortedOwnStatements = ownStatements
            .sorted { ($0.statement.getExpiry() ?? 0) > ($1.statement.getExpiry() ?? 0) }
        let sortedPeerStatements = peerStatements
            .sorted { ($0.statement.getExpiry() ?? 0) > ($1.statement.getExpiry() ?? 0) }

        let ownStatementDataList = makeStatementDataList(from: sortedOwnStatements)
        let expiry = priorityProvider.initialExpiry(from: ownStatementDataList.map(\.statement))

        logger?.debug("Resolved priority: \(expiry)")

        isInitializing = false

        if let pollingError = [ownError, peerError].compactMap({ $0 }).first(where: Self.isFatal) {
            logger?.error("Session initialization failed: \(pollingError)")
            delegate?.sessionInitializer(
                self,
                didFailToInitializeWith: .init(
                    error: makeInitializationError(pollingError: pollingError),
                    expiry: expiry
                )
            )
            return
        }

        let peerStatementDataList = makeStatementDataList(from: sortedPeerStatements)

        delegate?.sessionInitializer(
            self,
            didInitializeWith: .init(
                outgoingState: makeOutgoingState(
                    ownStatementDataList: ownStatementDataList,
                    peerStatementDataList: peerStatementDataList
                ),
                incomingState: makeIncomingState(
                    ownStatementDataList: ownStatementDataList,
                    peerStatementDataList: peerStatementDataList
                ),
                priority: expiry
            )
        )
    }

    /// Anyone can publish under a topic, so statements that fail to decrypt or decode are skipped, never fatal.
    func makeStatementDataList(
        from statements: [PeerSessionStatement]
    ) -> [DecodedStatement] {
        statements.compactMap { sessionStatement -> DecodedStatement? in
            let statement = sessionStatement.statement

            guard let payload = statement.getScaleEncodedPayload() else {
                return nil
            }

            let result: StatementDataDecodingResult<Message>

            do {
                result = try statementDataCoder.decodeFromScaleEncodedPayload(
                    payload,
                    senderAccountId: sessionStatement.topicAccountId,
                    route: sessionStatement.route
                )
            } catch {
                logger?.debug("Skipping undecodable statement on route \(sessionStatement.route): \(error)")
                return nil
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

    func makeIncomingState(
        ownStatementDataList: [DecodedStatement],
        peerStatementDataList: [DecodedStatement]
    ) -> IncomingInitializationState<Message> {
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
    ) -> OutgoingInitializationState<Message> {
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

    // A statement the store accepted but we can't parse is junk anyone could post; the rest was still delivered.
    static func isFatal(_ pollingError: StatementSubscriptionError) -> Bool {
        switch pollingError {
        case .statementDecodingFailed:
            false
        case .other:
            true
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
}
