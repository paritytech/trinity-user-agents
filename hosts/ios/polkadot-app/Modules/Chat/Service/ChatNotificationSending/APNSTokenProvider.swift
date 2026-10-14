import Foundation
import UserNotifications
import UIKit
import CommonService

protocol APNSTokenProviding {
    var currentToken: Data? { get }

    func setDeviceToken(_ token: Data)
}

protocol APNSTokenObserving: BaseObservableStateStoreProtocol where RemoteState == Data {}

typealias APNSTokenManaging = APNSTokenObserving & APNSTokenProviding

final class APNSTokenProvider: BaseObservableStateStore<Data>, APNSTokenObserving, APNSTokenProviding {
    var currentToken: Data? {
        currentState
    }

    func setDeviceToken(_ token: Data) {
        mutex.lock()
        defer { mutex.unlock() }

        logger.debug("Did update remote token: \(token.toHex())")
        stateObservable.state = token
    }
}
