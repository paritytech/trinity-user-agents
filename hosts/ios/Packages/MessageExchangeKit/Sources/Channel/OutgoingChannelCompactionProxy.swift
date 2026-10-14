import Foundation
import Foundation_iOS
import StatementStore
import SDKLogger

final class OutgoingChannelCompactionProxy<M: MessageExchange.CodableMessage>: @unchecked Sendable {
    typealias Message = M

    struct CompactedMessages {
        let compacted: M
        let originals: [M]
    }

    weak var delegate: AnyOutgoingMessageChannelDelegate<M>?

    private let channel: AnyOutgoingMessageChannel<M>
    private let compacter: AnyMessageCompactor<M>
    private let routeSelector: PeerSessionRouteSelector<M>
    private let sizeValidator: OutgoingRequestSizeValidating
    private let statementDataCoder: StatementDataCoding
    private let workQueue: DispatchQueue
    private let logger: SDKLoggerProtocol?

    private var pendingMessages = [PeerSessionRoute: [Message]]() // arrived while compaction
    private var inChannelMessages = [PeerSessionRoute: [Message]]() // propagated to channel
    private var optimisticallyAcknowledged: [Message] = [] // got success callback before reaching channel
    private var compactionMappings: [CompactedMessages] = []
    private var compactionTasks = [PeerSessionRoute: Task<Void, Never>]()
    private var compactingRoutes = Set<PeerSessionRoute>()
    private var isActive = false

    init(
        channel: AnyOutgoingMessageChannel<M>,
        compacter: AnyMessageCompactor<M>,
        routeSelector: PeerSessionRouteSelector<M>,
        sizeValidator: OutgoingRequestSizeValidating,
        statementDataCoder: StatementDataCoding,
        workQueue: DispatchQueue,
        logger: SDKLoggerProtocol?
    ) {
        self.channel = channel
        self.compacter = compacter
        self.routeSelector = routeSelector
        self.sizeValidator = sizeValidator
        self.statementDataCoder = statementDataCoder
        self.workQueue = workQueue
        self.logger = logger
    }
}

// MARK: - OutgoingMessageChanneling

extension OutgoingChannelCompactionProxy: OutgoingMessageChanneling {
    func activate(restoringState requests: [PeerSessionRoute: OutgoingRequest<Message>]) {
        isActive = true

        channel.activate(restoringState: requests)

        for (route, request) in requests {
            inChannelMessages[route] = request.messages
        }

        flushAllPendingMessages()
    }

    func deactivate() {
        isActive = false

        channel.deactivate()
    }

    func handleResponse(
        _ response: MessageExchange.Response,
        route: PeerSessionRoute
    ) -> StatementHandlingStatus {
        channel.handleResponse(response, route: route)
    }

    func reset(route: PeerSessionRoute) {
        channel.reset(route: route)
    }

    func addMessagesToQueue(_ messages: [Message]) {
        var newMessages = [Message]()

        for message in messages {
            if checkNotDuplicate(message: message) {
                newMessages.append(message)
            } else {
                // mirror the queue's .ignored outcome so duplicates always
                // produce a success callback regardless of which layer drops them
                logger?.debug("Ignoring duplicate message already tracked by compaction proxy")
                delegate?.messageChannel(self, didFinishAddingMessageToQueue: message, withError: nil)
            }
        }

        guard !newMessages.isEmpty else {
            return
        }

        var affectedRoutes = [PeerSessionRoute]()

        for message in newMessages {
            let route = routeSelector.route(for: message)
            pendingMessages[route, default: []].append(message)

            if !affectedRoutes.contains(route) {
                affectedRoutes.append(route)
            }
        }

        for route in affectedRoutes {
            if compactingRoutes.contains(route) {
                // we are optimistic about messages to reach the queue
                for message in newMessages where routeSelector.route(for: message) == route {
                    optimisticallyAcknowledged.append(message)
                    delegate?.messageChannel(self, didFinishAddingMessageToQueue: message, withError: nil)
                }
            } else {
                flushPendingMessages(on: route)
            }
        }
    }
}

// MARK: - OutgoingMessageChannelDelegate

extension OutgoingChannelCompactionProxy: OutgoingMessageChannelDelegate, TypeErasedDelegateStoring {
    func messageChannel(
        _: any OutgoingMessageChanneling,
        didFinishAddingMessageToQueue message: Message,
        withError error: MessageExchange.AddToQueueError?
    ) {
        if let index = optimisticallyAcknowledged.firstIndex(where: { $0 == message }) {
            optimisticallyAcknowledged.remove(at: index)

            // success was already reported optimistically while compacting;
            // real errors still need to reach the delegate
            if error == nil {
                return
            }
        }

        if error != nil {
            // we are optimistic about messages to reach the queue
            // in case we couldn't compact a big message that reached the channel
            // and failed we need to filter it out
            removeFromInChannelMessages([message])
        }

        delegate?.messageChannel(self, didFinishAddingMessageToQueue: message, withError: error)
    }

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didPostMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        let resolvedMessages = resolveOriginalMessages(for: messages)

        delegate?.messageChannel(self, didPostMessages: resolvedMessages, withError: error)
    }

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didDeliverMessages messages: [Message],
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        let resolvedMessages = resolveOriginalMessages(for: messages)

        cleanupDeliveredMappings(for: messages)
        removeFromInChannelMessages(messages)
        optimisticallyAcknowledged.removeAll { messages.contains($0) }

        delegate?.messageChannel(self, didDeliverMessages: resolvedMessages, withError: error)
    }

    func messageChannel(
        _: any OutgoingMessageChanneling,
        didCompactMessages compactedMessage: Message,
        originalMessages: [Message]
    ) {
        delegate?.messageChannel(self, didCompactMessages: compactedMessage, originalMessages: originalMessages)
    }

    func statementSubmitFailed(with error: Error) {
        delegate?.statementSubmitFailed(with: error)
    }
}

// MARK: - Compaction

private extension OutgoingChannelCompactionProxy {
    func flushAllPendingMessages() {
        for route in pendingMessages.keys where !(pendingMessages[route] ?? []).isEmpty {
            flushPendingMessages(on: route)
        }
    }

    func flushPendingMessages(on route: PeerSessionRoute) {
        let pending = pendingMessages[route] ?? []

        guard !pending.isEmpty else {
            return
        }

        logger?.debug("Flushing pending messages on route \(route): \(pending.count)")

        let inChannel = inChannelMessages[route] ?? []

        if needsCompaction(for: inChannel + pending, route: route) {
            logger?.debug("Needs compaction on route \(route)")
            startCompaction(on: route)
        } else {
            logger?.debug("No need to compact on route \(route)")

            inChannelMessages[route, default: []].append(contentsOf: pending)
            pendingMessages[route] = []

            logger?.debug("Propagate messages to channel")
            channel.addMessagesToQueue(pending)
        }
    }

    func needsCompaction(for messages: [Message], route: PeerSessionRoute) -> Bool {
        guard !messages.isEmpty else {
            return false
        }

        do {
            let payload = try statementDataCoder.encodeToScaleEncodedPayload(
                .request(.init(requestId: UUID().uuidString, messages: messages)),
                route: route
            )

            return !sizeValidator.scaleEncodedPayloadFits(payload)
        } catch {
            logger?.warning("Compaction size probe failed on route \(route), skipping compaction: \(error)")
            return false
        }
    }

    func startCompaction(on route: PeerSessionRoute) {
        guard isActive else {
            logger?.debug("Postponing compaction as the channel is not active")
            return
        }

        compactingRoutes.insert(route)

        let currentInChannel = inChannelMessages[route] ?? []
        let currentPending = pendingMessages[route] ?? []
        let messagesToCompact = currentInChannel + currentPending

        logger?.debug("Starting compaction for \(messagesToCompact.count) messages on route \(route)")

        channel.reset(route: route)
        pendingMessages[route] = []
        inChannelMessages[route] = []

        compactionTasks[route]?.cancel()
        compactionTasks[route] = Task { [compacter, weak self] in
            do {
                let compactedMessage = try await compacter.compact(messages: messagesToCompact)

                try Task.checkCancellation()

                self?.logger?.debug("Compaction succeeded")

                self?.workQueue.async {
                    self?.handleCompactionSuccess(
                        compactedMessage: compactedMessage,
                        originalMessages: messagesToCompact,
                        route: route
                    )
                }
            } catch {
                guard !Task.isCancelled else { return }

                self?.logger?.error("Compaction failed: \(error)")

                self?.workQueue.async {
                    self?.handleCompactionFailure(
                        originalMessages: messagesToCompact,
                        route: route
                    )
                }
            }
        }
    }

    func handleCompactionSuccess(
        compactedMessage: Message,
        originalMessages: [Message],
        route: PeerSessionRoute
    ) {
        compactingRoutes.remove(route)
        compactionTasks[route] = nil

        let compactedRoute = routeSelector.route(for: compactedMessage)

        if compactedRoute != route {
            logger?.warning("Compacted message routed to \(compactedRoute) while originals used \(route)")
        }

        inChannelMessages[compactedRoute, default: []].append(compactedMessage)
        optimisticallyAcknowledged.removeAll { originalMessages.contains($0) }
        compactionMappings.append(.init(compacted: compactedMessage, originals: originalMessages))

        delegate?.messageChannel(
            self,
            didCompactMessages: compactedMessage,
            originalMessages: originalMessages
        )

        channel.addMessagesToQueue([compactedMessage])

        pendingMessages[route] = (pendingMessages[route] ?? []).filter { !originalMessages.contains($0) }
        flushPendingMessages(on: route)
    }

    func handleCompactionFailure(
        originalMessages: [Message],
        route: PeerSessionRoute
    ) {
        logger?.error("Compaction failed. Sending messages uncompacted.")

        compactingRoutes.remove(route)
        compactionTasks[route] = nil

        let accumulatedPending = (pendingMessages[route] ?? []).filter { !originalMessages.contains($0) }

        let allMessages = originalMessages + accumulatedPending
        pendingMessages[route] = []
        inChannelMessages[route, default: []].append(contentsOf: allMessages)

        channel.addMessagesToQueue(allMessages)
    }

    func resolveOriginalMessages(for messages: [Message]) -> [Message] {
        var resolved = [Message]()

        for message in messages {
            if let mapping = compactionMappings.first(where: { $0.compacted == message }) {
                resolved.append(contentsOf: mapping.originals)
            }

            resolved.append(message)
        }

        return resolved
    }

    func cleanupDeliveredMappings(for messages: [Message]) {
        compactionMappings.removeAll { mapping in
            messages.contains(mapping.compacted)
        }
    }

    func removeFromInChannelMessages(_ messages: [Message]) {
        for route in inChannelMessages.keys {
            inChannelMessages[route]?.removeAll { messages.contains($0) }
        }
    }

    func checkNotDuplicate(message: Message) -> Bool {
        let isPending = pendingMessages.values.contains { $0.contains(message) }
        let isInChannel = inChannelMessages.values.contains { $0.contains(message) }

        return !isPending && !isInChannel
    }
}
