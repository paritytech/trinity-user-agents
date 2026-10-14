import Foundation
import AsyncExtensions

enum CallMessageEvent: Equatable {
    case offerDelivered
    case closedSent
}

protocol CallMessageEventProviding {
    var messageEvents: AnyAsyncSequence<CallMessageEvent> { get }
}
