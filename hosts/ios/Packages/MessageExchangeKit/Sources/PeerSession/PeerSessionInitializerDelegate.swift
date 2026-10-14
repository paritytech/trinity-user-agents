import Foundation
import Foundation_iOS

protocol PeerSessionInitializerDelegate: AnyObject {
    associatedtype Message: MessageExchange.CodableMessage

    func sessionInitializer(
        _ initializer: PeerSessionInitializing,
        didInitializeWith result: SessionInitializationSuccess<Message>
    )

    func sessionInitializer(
        _ initializer: PeerSessionInitializing,
        didFailToInitializeWith result: SessionInitializationFailure
    )
}

// MARK: - Type Erasure Implementation

final class AnyPeerSessionInitializerDelegate<M: MessageExchange.CodableMessage>: PeerSessionInitializerDelegate {
    typealias Message = M

    private let didInitializeClosure: (
        any PeerSessionInitializing,
        SessionInitializationSuccess<M>
    ) -> Void

    private let didFailClosure: (
        any PeerSessionInitializing,
        SessionInitializationFailure
    ) -> Void

    init<
        D: PeerSessionInitializerDelegate & TypeErasedDelegateStoring
    >(_ targetDelegate: D) where D.Message == M {
        didInitializeClosure = { [weak targetDelegate] initializer, result in
            targetDelegate?.sessionInitializer(
                initializer,
                didInitializeWith: result
            )
        }

        didFailClosure = { [weak targetDelegate] initializer, result in
            targetDelegate?.sessionInitializer(
                initializer,
                didFailToInitializeWith: result
            )
        }

        targetDelegate.storeErasedType(instance: self)
    }

    func sessionInitializer(
        _ initializer: PeerSessionInitializing,
        didInitializeWith result: SessionInitializationSuccess<M>
    ) {
        didInitializeClosure(initializer, result)
    }

    func sessionInitializer(
        _ initializer: PeerSessionInitializing,
        didFailToInitializeWith result: SessionInitializationFailure
    ) {
        didFailClosure(initializer, result)
    }
}
