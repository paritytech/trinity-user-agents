import Foundation
import StatementStore
import SDKLogger

final class OutgoingRequestQueue<M: MessageExchange.CodableMessage> {
    typealias Message = M

    private let statementDataCoder: StatementDataCoding
    private let routeSelector: PeerSessionRouteSelector<M>
    private let sizeValidator: OutgoingRequestSizeValidating
    private let supportedRoutes: [PeerSessionRoute]
    private let logger: SDKLoggerProtocol?

    private var queuedMessages = [PeerSessionRoute: [Message]]()

    var currentRequests = [PeerSessionRoute: OutgoingRequest<Message>]()

    init(
        statementDataCoder: StatementDataCoding,
        routeSelector: PeerSessionRouteSelector<M>,
        sizeValidator: OutgoingRequestSizeValidating,
        supportedRoutes: [PeerSessionRoute],
        logger: SDKLoggerProtocol?
    ) {
        self.statementDataCoder = statementDataCoder
        self.routeSelector = routeSelector
        self.sizeValidator = sizeValidator
        self.supportedRoutes = supportedRoutes
        self.logger = logger
    }
}

extension OutgoingRequestQueue: OutgoingRequestQueueing {
    func reset(route: PeerSessionRoute) {
        currentRequests[route] = nil
        queuedMessages[route] = []
    }

    func attemptRequestExtensionFromQueue() -> Set<PeerSessionRoute> {
        var messagesToRetry = [Message]()

        for route in supportedRoutes {
            messagesToRetry.append(contentsOf: queuedMessages[route] ?? [])
            queuedMessages[route] = []
        }

        guard !messagesToRetry.isEmpty else {
            return []
        }

        var extendedRoutes = Set<PeerSessionRoute>()

        for message in messagesToRetry {
            let result = performMessageAdd(message, isChannelActive: true)

            if case let .success(successOutcome) = result,
               case let .appendedToCurrentRequest(route) = successOutcome {
                extendedRoutes.insert(route)
            }
        }

        return extendedRoutes
    }

    func addMessages(
        _ messages: [Message],
        isChannelActive: Bool
    ) -> [Result<
        MessageExchange.AddToQueueResult,
        MessageExchange.AddToQueueError
    >] {
        messages.map { performMessageAdd($0, isChannelActive: isChannelActive) }
    }

    func dequeueMessagesForNewRequest() -> OutgoingRequest<M>? {
        guard let route = routeForNextRequest() else {
            return nil
        }

        let queue = queuedMessages[route] ?? []
        let requestId = UUID().uuidString
        var messages = [M]()
        var scaleEncodedData = Data()
        var index = 0

        while true {
            do {
                guard routeSelector.route(for: queue[index]) == route else {
                    break
                }

                let newMessages = messages + [queue[index]]

                let newData = try statementDataCoder.encodeToScaleEncodedPayload(
                    .request(.init(
                        requestId: requestId,
                        messages: newMessages
                    )),
                    route: route
                )

                if sizeValidator.scaleEncodedPayloadFits(newData) {
                    messages = newMessages
                    scaleEncodedData = newData
                    index += 1
                } else {
                    break
                }

                if index >= queue.count {
                    break
                }
            } catch {
                assertionFailure("Unexpected error: \(error)")
                logger?.error("Unexpected error: \(error)")
                break
            }
        }

        guard !messages.isEmpty, !scaleEncodedData.isEmpty else {
            logger?.error("Data is empty")
            return nil
        }

        queuedMessages[route]?.removeFirst(index)

        logger?.debug("Added \(messages.count) for size \(scaleEncodedData.count)")

        return .init(
            requestId: requestId,
            messages: messages,
            scaleEncodedPayload: scaleEncodedData,
            route: route
        )
    }
}

private extension OutgoingRequestQueue {
    func makeError(error: Error) -> MessageExchange.AddToQueueError {
        guard let encodingError = error as? StatementDataEncodingError else {
            assertionFailure("Unexpected error: \(error)")
            logger?.error("Unexpected error: \(error)")
            return .encodingFailed
        }
        switch encodingError {
        case .encodingFailed:
            return .encodingFailed
        case .encryptionFailed:
            return .encryptionFailed
        }
    }

    func performMessageAdd(
        _ message: Message,
        isChannelActive: Bool
    ) -> Result<
        MessageExchange.AddToQueueResult,
        MessageExchange.AddToQueueError
    > {
        do {
            let route = routeSelector.route(for: message)

            guard supportedRoutes.contains(route) else {
                logger?.error("Unsupported route \(route)")
                return .failure(.unsupportedRoute(route))
            }

            // TODO: We use the randow request id and a single value array to check the size
            // Should be redone properly
            let scaleEncodedPayload = try statementDataCoder.encodeToScaleEncodedPayload(
                .request(.init(
                    requestId: UUID().uuidString,
                    messages: [message]
                )),
                route: route
            )

            if sizeValidator.scaleEncodedPayloadFits(scaleEncodedPayload) {
                return .success(continueAddingToQueue(
                    for: message,
                    isChannelActive: isChannelActive
                ))
            } else {
                return .failure(
                    .messageTooBig(
                        maxSize: sizeValidator.maxPayloadSize,
                        actualSize: scaleEncodedPayload.count
                    )
                )
            }
        } catch {
            let error = makeError(error: error)
            return .failure(error)
        }
    }

    func continueAddingToQueue(
        for message: Message,
        isChannelActive: Bool
    ) -> MessageExchange.AddToQueueResult {
        let route = routeSelector.route(for: message)
        let queue = queuedMessages[route] ?? []

        guard
            isChannelActive,
            queue.isEmpty,
            let currentRequest = currentRequests[route]
        else {
            return queueMessage(message, route: route)
        }

        if currentRequest.messages.contains(message) {
            return .ignored
        }

        let newMessages = currentRequest.messages + [message]

        do {
            let newRequestId = UUID().uuidString

            let scaleEncodedData = try statementDataCoder.encodeToScaleEncodedPayload(
                .request(.init(
                    requestId: newRequestId,
                    messages: newMessages
                )),
                route: route
            )

            if sizeValidator.scaleEncodedPayloadFits(scaleEncodedData) {
                currentRequests[route] = .init(
                    requestId: newRequestId,
                    messages: newMessages,
                    scaleEncodedPayload: scaleEncodedData,
                    route: route
                )
                return .appendedToCurrentRequest(route: route)
            } else {
                return queueMessage(message, route: route)
            }
        } catch {
            logger?.error("Encoding error: \(error)")
            return queueMessage(message, route: route)
        }
    }

    func queueMessage(
        _ message: Message,
        route: PeerSessionRoute
    ) -> MessageExchange.AddToQueueResult {
        let routeQueue = queuedMessages[route] ?? []

        if routeQueue.contains(message) {
            return .ignored
        } else {
            queuedMessages[route, default: []].append(message)
            return .queued
        }
    }

    func routeForNextRequest() -> PeerSessionRoute? {
        // The supported route order is the dequeue priority. A current request
        // blocks only its own route, so another route can still send.
        supportedRoutes.first { route in
            currentRequests[route] == nil && !(queuedMessages[route]?.isEmpty ?? true)
        }
    }
}
