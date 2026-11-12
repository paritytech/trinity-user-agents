import Foundation
import Foundation_iOS

final class MockApplicationHandler: ApplicationHandlerProtocol {
    weak var delegate: ApplicationHandlerDelegate?

    func simulateDidEnterBackground() {
        delegate?.didReceiveDidEnterBackground?(notification: Notification(name: .init("test.background")))
    }

    func simulateWillEnterForeground() {
        delegate?.didReceiveWillEnterForeground?(notification: Notification(name: .init("test.foreground")))
    }

    func simulateDidBecomeActive() {
        delegate?.didReceiveDidBecomeActive?(notification: Notification(name: .init("test.active")))
    }
}
