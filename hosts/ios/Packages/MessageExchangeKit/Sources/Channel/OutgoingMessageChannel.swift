import Foundation
import Operation_iOS
import StatementStore
import SDKLogger

final class OutgoingMessageChannel<M: MessageExchange.CodableMessage>: @unchecked Sendable {
    typealias Message = M

    weak var delegate: AnyOutgoingMessageChannelDelegate<M>?

    private let workQueue: DispatchQueue
    private let routeContexts: PeerSessionRoute.Contexts
    private let submitter: StatementStoreSubmitting
    private let signer: StatementStoreSigning
    private let preSendHandler: AnyPeerSessionPreSendHandler<M>
    private let priorityProvider: PeerSessionPriorityProviding
    private let requestQueue: AnyOutgoingRequestQueue<M>
    private let operationQueue: OperationQueue
    private let logger: SDKLoggerProtocol?
    private var submissionTasks = [PeerSessionRoute: Task<Void, Never>]()

    private var isActive = false

    init(
        workQueue: DispatchQueue,
        routeContexts: PeerSessionRoute.Contexts,
        submitter: StatementStoreSubmitting,
        signer: StatementStoreSigning,
        preSendHandler: AnyPeerSessionPreSendHandler<M>,
        priorityProvider: PeerSessionPriorityProviding,
        requestQueue: AnyOutgoingRequestQueue<M>,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol?
    ) {
        self.workQueue = workQueue
        self.routeContexts = routeContexts
        self.submitter = submitter
        self.signer = signer
        self.preSendHandler = preSendHandler
        self.priorityProvider = priorityProvider
        self.requestQueue = requestQueue
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension OutgoingMessageChannel: OutgoingMessageChanneling {
    func handleResponse(
        _ response: MessageExchange.Response,
        route: PeerSessionRoute
    ) -> StatementHandlingStatus {
        assert(delegate != nil, "Delegate should not be nil")

        guard let currentRequest = requestQueue.currentRequests[route] else {
            logger?.debug("Ignoring response \(response.requestId) on route \(route); current request is nil")
            return false
        }

        guard response.requestId == currentRequest.requestId else {
            logger?.debug(
                "Ignoring response \(response.requestId) on route \(route); current request is \(currentRequest.requestId)"
            )
            return false
        }

        logger?.debug("Got new response \(response)")

        if response.responseCode.isSuccess {
            logger?.debug("Message was delivered successfully")
            handleMessageDeliveringFinish(with: nil, route: route)
        } else {
            logger?.debug("Got failed response: \(response.responseCode)")
            handleMessageDeliveringFinish(with: .gotFailedResponse(response.responseCode), route: route)
        }

        return true
    }

    func activate(restoringState requests: [PeerSessionRoute: OutgoingRequest<Message>]) {
        requestQueue.currentRequests = requests
        isActive = true

        logger?.debug("Trying to send messages as channel became active")
        let extendedRoutes = requestQueue.attemptRequestExtensionFromQueue()
        for route in extendedRoutes {
            resendCurrentRequest(on: route)
        }
        sendQueuedRequests()
    }

    func deactivate() {
        isActive = false
    }

    func addMessagesToQueue(_ messages: [Message]) {
        guard !messages.isEmpty else {
            return
        }

        let results = requestQueue.addMessages(messages, isChannelActive: isActive)

        var appendedRoutes = Set<PeerSessionRoute>()
        var didQueue = false

        for (message, result) in zip(messages, results) {
            switch result {
            case let .success(outcome):
                notifyAddedToQueue(outcome, for: message)
                switch outcome {
                case let .appendedToCurrentRequest(route):
                    appendedRoutes.insert(route)
                case .queued:
                    didQueue = true
                case .ignored:
                    break
                }
            case let .failure(error):
                notifyAddToQueueError(error, for: message)
            }
        }

        for route in appendedRoutes {
            resendCurrentRequest(on: route)
        }

        if didQueue {
            sendQueuedRequests()
        }
    }

    func reset(route: PeerSessionRoute) {
        submissionTasks[route]?.cancel()
        submissionTasks[route] = nil
        requestQueue.reset(route: route)
    }
}

private extension OutgoingMessageChannel {
    func notifyAddedToQueue(_ outcome: MessageExchange.AddToQueueResult, for message: Message) {
        logger?.debug("Added message to queue (\(outcome))")

        delegate?.messageChannel(
            self,
            didFinishAddingMessageToQueue: message,
            withError: nil
        )
    }

    func notifyAddToQueueError(_ error: MessageExchange.AddToQueueError, for message: Message) {
        logger?.error("Failed to add message to queue: \(error)")

        delegate?.messageChannel(
            self,
            didFinishAddingMessageToQueue: message,
            withError: error
        )
    }

    func resendCurrentRequest(on route: PeerSessionRoute) {
        guard isActive else {
            logger?.debug("Channel is not in the active state")
            return
        }

        guard let request = requestQueue.currentRequests[route] else {
            logger?.debug("No current request to resend on route \(route)")
            return
        }

        startSending(request)
    }

    func sendQueuedRequests() {
        guard isActive else {
            logger?.debug("Channel is not in the active state")
            return
        }

        var didStartRequest = false

        while let request = requestQueue.dequeueMessagesForNewRequest() {
            didStartRequest = true
            startSending(request)
        }

        if !didStartRequest {
            logger?.debug("No messages in queue")
        }
    }

    func startSending(_ request: OutgoingRequest<Message>) {
        for message in request.messages {
            preSendHandler.handlePreSend(message: message)
        }

        requestQueue.currentRequests[request.route] = request
        sendOutgoingRequest(request)
    }

    func handleMessagePostingFinish(
        with error: Error?,
        route: PeerSessionRoute
    ) {
        let messages = requestQueue.currentRequests[route]?.messages ?? []

        if let error {
            logger?.error("Failed to post messages on route \(route): \(error.localizedDescription)")
            finishRequestSending(route: route)
        } else {
            logger?.debug("Successfully posted messages on route \(route), waiting for delivering")
        }

        delegate?.messageChannel(
            self,
            didPostMessages: messages,
            withError: error.map { .failedToPost($0) }
        )
    }

    func handleMessageDeliveringFinish(
        with error: MessageExchange.OutgoingMessageError?,
        route: PeerSessionRoute
    ) {
        let messages = requestQueue.currentRequests[route]?.messages ?? []

        if let error {
            logger?.error("Failed to deliver messages on route \(route): \(error.localizedDescription)")
        } else {
            logger?.debug("Successfully delivered messages on route \(route)")
        }

        finishRequestSending(route: route)

        delegate?.messageChannel(
            self,
            didDeliverMessages: messages,
            withError: error
        )
    }

    func finishRequestSending(route: PeerSessionRoute) {
        requestQueue.currentRequests[route] = nil
        submissionTasks[route] = nil

        logger?.debug("Finished request on route \(route), trying to send more messages")
        sendQueuedRequests()
    }

    func sendOutgoingRequest(_ outgoingRequest: OutgoingRequest<Message>) {
        let expiry = priorityProvider.incrementedExpiry()
        let requestId = outgoingRequest.requestId
        let route = outgoingRequest.route
        let routeContext = routeContexts.context(for: route)

        let builder = StatementSubmitParametersBuilder(
            signer: signer,
            logger: logger
        )
        .addTopic1(routeContext.sessionId.own)
        .addChannel(routeContext.requestChannelId)
        .addExpiry(expiry)
        .addScaleEncodedPayload(outgoingRequest.scaleEncodedPayload)

        logger?.debug("Going to send request \(requestId) with priority \(expiry)")

        submissionTasks[route]?.cancel()

        submissionTasks[route] = Task { [weak self] in
            do {
                try await self?.submitter.submitStatement(with: builder)
                self?.logger?.debug("Request \(requestId) sent successfully")

                self?.workQueue.async {
                    self?.handleMessagePostingFinish(with: nil, route: route)
                }

            } catch {
                guard !Task.isCancelled else {
                    return
                }

                self?.logger?.error("Failed to send request \(requestId): \(error)")

                self?.workQueue.async {
                    self?.handleMessagePostingFinish(with: error, route: route)
                    self?.delegate?.statementSubmitFailed(with: error)
                }
            }
        }
    }
}
