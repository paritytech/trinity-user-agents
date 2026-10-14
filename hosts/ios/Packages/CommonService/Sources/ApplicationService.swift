import Foundation

public protocol ApplicationServiceProtocol {
    func setup()
    func throttle()
}

public protocol AsyncApplicationServicing {
    func setup() async
    func throttle() async
}
