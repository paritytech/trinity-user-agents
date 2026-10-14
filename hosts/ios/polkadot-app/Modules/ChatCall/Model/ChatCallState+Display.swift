import Foundation

extension ChatCallState {
    var statusText: String? {
        switch self {
        case .contacting:
            String(localized: .chatCallStatusContacting)
        case .ringing,
             .connecting:
            String(localized: .chatCallStatusRinging)
        case .ended:
            String(localized: .chatCallStatusEnded)
        case .failed:
            String(localized: .chatCallStatusFailed)
        case .connected:
            nil
        }
    }

    var showsDuration: Bool {
        if case .connected = self {
            return true
        }
        return false
    }

    var isRingingState: Bool {
        switch self {
        case .contacting,
             .ringing:
            true
        default:
            false
        }
    }
}

enum ChatCallDurationFormatter {
    static func string(from interval: TimeInterval, locale: Locale = .current) -> String {
        let total = max(interval, 0)
        let resource = total >= .secondsInHour
            ? DateComponentsFormatter.fullTime
            : DateComponentsFormatter.minuteSeconds
        return resource.value(for: locale).string(from: total) ?? ""
    }
}
