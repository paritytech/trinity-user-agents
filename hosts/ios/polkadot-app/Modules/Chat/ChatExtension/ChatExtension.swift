import Foundation

enum ChatExtension {
    // swiftlint:disable:next type_name
    typealias Id = String
    typealias ActionId = String

    enum ProcessingResult {
        case skipped
        case processed
    }
}

enum MessageDeliveryDelay {
    case immediate
    case humanInteraction
    case custom(TimeInterval)
}

extension MessageDeliveryDelay {
    var delayDuration: TimeInterval {
        switch self {
        case .immediate:
            0
        case .humanInteraction:
            1
        case let .custom(delay):
            delay
        }
    }

    func delay() async {
        guard delayDuration > 0 else {
            return
        }

        try? await Task.sleep(nanoseconds: UInt64(TimeInterval(NSEC_PER_SEC) * delayDuration))
    }
}
