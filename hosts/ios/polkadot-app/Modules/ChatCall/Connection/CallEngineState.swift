import Foundation

enum CallEngineState: Equatable {
    case contacting // waiting for request(offer) delivery
    case waiting // offer delivered, waiting for answer
    case connecting
    case connected
    case disconnected
    case failed
}
