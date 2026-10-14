import Foundation
import StatementStore

typealias PeerSessionStatementHandler = (PeerSessionStatement) -> StatementHandlingStatus

protocol PeerSessionStatementSubscribing {
    func start(handler: @escaping PeerSessionStatementHandler)
    func stop()
    func resetSeenHashes()
    func fetchOnce(
        handler: @escaping PeerSessionStatementHandler,
        completion: ((StatementSubscriptionError?) -> Void)?
    )
}

final class PeerSessionStatementSubscription: PeerSessionStatementSubscribing {
    struct Input {
        let route: PeerSessionRoute
        let subscription: StatementSubscribing
    }

    private let inputs: [Input]
    private let workQueue: DispatchQueue

    init(
        inputs: [Input],
        workQueue: DispatchQueue
    ) {
        self.inputs = inputs
        self.workQueue = workQueue
    }

    func start(handler: @escaping PeerSessionStatementHandler) {
        inputs.forEach { input in
            input.subscription.start { statement in
                handler(.init(statement: statement, route: input.route))
            }
        }
    }

    func stop() {
        inputs.forEach { $0.subscription.stop() }
    }

    func resetSeenHashes() {
        inputs.forEach { $0.subscription.resetSeenHashes() }
    }

    func fetchOnce(
        handler: @escaping PeerSessionStatementHandler,
        completion: ((StatementSubscriptionError?) -> Void)?
    ) {
        guard !inputs.isEmpty else {
            completion?(nil)
            return
        }

        let group = DispatchGroup()
        var resultError: StatementSubscriptionError?

        inputs.forEach { input in
            group.enter()
            input.subscription.fetchOnce(handler: { statement in
                handler(.init(statement: statement, route: input.route))
            }, completion: { error in
                if resultError == nil {
                    resultError = error
                }
                group.leave()
            })
        }

        group.notify(queue: workQueue) {
            completion?(resultError)
        }
    }
}
