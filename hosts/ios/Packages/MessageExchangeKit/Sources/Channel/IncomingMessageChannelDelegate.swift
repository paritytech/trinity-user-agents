import Foundation
import Foundation_iOS

protocol MessageChannelDelegate: AnyObject {
    func statementSubmitFailed(with error: Error)
}

protocol IncomingMessageChannelDelegate: MessageChannelDelegate {
    associatedtype Message: MessageExchange.CodableMessage
}

// MARK: - Type Erasure Implementation

final class AnyIncomingMessageChannelDelegate<M: MessageExchange.CodableMessage>: IncomingMessageChannelDelegate {
    typealias Message = M

    private let statementSubmitFailedClosure: (Error) -> Void

    init<
        D: IncomingMessageChannelDelegate & TypeErasedDelegateStoring
    >(_ targetDelegate: D) where D.Message == M {
        statementSubmitFailedClosure = { [weak targetDelegate] error in
            targetDelegate?.statementSubmitFailed(with: error)
        }

        targetDelegate.storeErasedType(instance: self)
    }

    func statementSubmitFailed(with error: Error) {
        statementSubmitFailedClosure(error)
    }
}
