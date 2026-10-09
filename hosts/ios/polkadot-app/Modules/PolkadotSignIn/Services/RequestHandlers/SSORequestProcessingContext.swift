import Foundation
import Products

actor SSORequestProcessingContext<Message: HostMessageIdentifiable> {
    struct PendingRequest {
        let message: Message
        let host: PolkadotSignInHost
    }

    private var pendingRequests: [PendingRequest] = []
    private var activeTask: Task<Void, Never>?
    private var activeMessageId: String?
    private let handlers: [any SSORequestHandling<Message>]
    private let withdrawnRequests: SSOWithdrawnRequests
    private let logger: LoggerProtocol

    init(
        handlers: [any SSORequestHandling<Message>],
        withdrawnRequests: SSOWithdrawnRequests = SSOWithdrawnRequests(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.handlers = handlers
        self.withdrawnRequests = withdrawnRequests
        self.logger = logger
    }

    func enqueue(message: Message, from host: PolkadotSignInHost) {
        guard !withdrawnRequests.contains(message.messageId) else {
            logger.info("Dropping withdrawn request \(message.messageId)")
            return
        }

        let pending = PendingRequest(message: message, host: host)

        if activeTask == nil {
            logger.info("Processing task right away")
            startProcessing(pending)
        } else {
            pendingRequests.append(pending)
            logger.info("Queued request \(message.messageId), queue size: \(pendingRequests.count)")
        }
    }

    /// Hands `message` to its handler now, beside the request the queue is
    /// serving.
    func processImmediately(message: Message, from host: PolkadotSignInHost) async {
        await process(PendingRequest(message: message, host: host))
    }

    /// Withdraws a request: a queued one is dropped, a running one is
    /// cancelled and its prompts close, and one not yet received never runs.
    func withdraw(requestId: String) {
        withdrawnRequests.insert(requestId)

        if let index = pendingRequests.firstIndex(where: { $0.message.messageId == requestId }) {
            pendingRequests.remove(at: index)
            logger.info("Dropped queued request \(requestId) on withdrawal")
        } else if activeMessageId == requestId {
            activeTask?.cancel()
            logger.info("Cancelled running request \(requestId) on withdrawal")
        } else {
            logger.info("Remembered withdrawal of request \(requestId), which is neither queued nor running")
        }
    }

    func cancelAll() {
        activeTask?.cancel()
        activeTask = nil
        activeMessageId = nil
        pendingRequests.removeAll()
    }
}

private extension SSORequestProcessingContext {
    func startProcessing(_ request: PendingRequest) {
        let scope = PromptPresentationScope()
        activeMessageId = request.message.messageId
        activeTask = Task { [weak self] in
            await scope.run {
                await self?.process(request)
            }
            await self?.processNext()
        }
    }

    func process(_ request: PendingRequest) async {
        for handler in handlers where handler.canHandle(request.message) {
            logger.info("Processing \(request.message.messageId) with \(type(of: handler))")
            await handler.handle(
                message: request.message,
                from: request.host
            )
            return
        }

        logger.warning("No handler for message \(request.message.messageId)")
    }

    func processNext() {
        activeTask = nil
        activeMessageId = nil

        guard !pendingRequests.isEmpty else {
            return
        }

        let next = pendingRequests.removeFirst()
        logger.debug("Dequeuing \(next.message.messageId), remaining: \(pendingRequests.count)")
        startProcessing(next)
    }
}
