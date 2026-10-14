import Foundation
import CryptoKit
import Operation_iOS
import StatementStore
import SDKLogger

final class IncomingMessageChannel<M: MessageExchange.CodableMessage>: @unchecked Sendable {
    typealias Message = M

    weak var delegate: AnyIncomingMessageChannelDelegate<M>?

    private let workQueue: DispatchQueue
    private let routeContexts: PeerSessionRoute.Contexts
    private let submitter: StatementStoreSubmitting
    private let signer: StatementStoreSigning
    private let priorityProvider: PeerSessionPriorityProviding
    private let statementDataCoder: StatementDataCoding
    private let operationQueue: OperationQueue
    private let logger: SDKLoggerProtocol?

    private var submissionTasks = [PeerSessionRoute: Task<Void, Never>]()

    init(
        workQueue: DispatchQueue,
        routeContexts: PeerSessionRoute.Contexts,
        submitter: StatementStoreSubmitting,
        signer: StatementStoreSigning,
        priorityProvider: PeerSessionPriorityProviding,
        statementDataCoder: StatementDataCoding,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol?
    ) {
        self.workQueue = workQueue
        self.routeContexts = routeContexts
        self.submitter = submitter
        self.signer = signer
        self.priorityProvider = priorityProvider
        self.statementDataCoder = statementDataCoder
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension IncomingMessageChannel: IncomingMessageChanneling {
    func sendResponse(
        with responseCode: MessageExchange.ResponseCode,
        forRequestId requestId: String,
        route: PeerSessionRoute
    ) {
        assert(delegate != nil, "Delegate should not be nil")

        guard let scaleEncodedPayload = makeScaleEncodedPayload(
            response: .init(
                requestId: requestId,
                responseCode: responseCode
            ),
            route: route
        )
        else {
            return
        }

        let expiry = priorityProvider.incrementedExpiry()
        let routeContext = routeContexts.context(for: route)

        let builder = StatementSubmitParametersBuilder(
            signer: signer,
            logger: logger
        )
        .addTopic1(routeContext.sessionId.own)
        .addChannel(routeContext.responseChannelId)
        .addExpiry(expiry)
        .addScaleEncodedPayload(scaleEncodedPayload)

        logger?.debug("Going to send \(responseCode) for request \(requestId) with priority \(expiry)")

        // Same-route responses share the same response statement. Replacing a response
        // on one route must not cancel another route's response submission.
        submissionTasks[route]?.cancel()
        submissionTasks[route] = Task { [weak self] in
            do {
                try await self?.submitter.submitStatement(with: builder)
                self?.logger?.debug("\(responseCode) sent for request \(requestId)")
            } catch {
                guard !Task.isCancelled else { return }
                self?.logger?.error("Failed to send \(responseCode) for request \(requestId): \(error)")
                self?.workQueue.async {
                    self?.delegate?.statementSubmitFailed(with: error)
                }
            }
        }
    }
}

private extension IncomingMessageChannel {
    func makeScaleEncodedPayload(
        response: MessageExchange.Response,
        route: PeerSessionRoute
    ) -> Data? {
        do {
            let statementData = StatementData<Message>.response(response)
            return try statementDataCoder.encodeToScaleEncodedPayload(statementData, route: route)
        } catch {
            logger?.error("Failed to encode response: \(error)")
            return nil
        }
    }
}
