import Foundation
import SubstrateSdk

extension Chat {
    enum RequestStatus: Equatable {
        enum Incoming: UInt8, Equatable {
            case new
            case declined
            case accepted
            case cancelled
        }

        enum Outgoing: Equatable {
            case pending
            case cancelled
        }

        case incoming(Incoming)
        case outgoing(Outgoing)

        var statusClass: RequestStatusClass {
            switch self {
            case .incoming:
                .incoming
            case .outgoing:
                .outgoing
            }
        }
    }

    enum RequestStatusClass: Equatable {
        case outgoing
        case incoming
    }

    struct Request: Equatable {
        let requestId: String
        let contactAccountId: AccountId
        let timestamp: UInt64
        let status: RequestStatus
        let message: Chat.LocalMessage?

        var isOutgoing: Bool {
            switch status {
            case .incoming:
                false
            case .outgoing:
                true
            }
        }

        var isIncoming: Bool {
            !isOutgoing
        }
    }
}

extension Chat.RequestStatus {
    init?(rawValue: Int16) {
        switch rawValue {
        case 0:
            self = .incoming(.new)
        case 3:
            self = .incoming(.accepted)
        case 4:
            self = .incoming(.declined)
        case 5:
            self = .outgoing(.pending)
        case 6:
            self = .incoming(.cancelled)
        case 7:
            self = .outgoing(.cancelled)
        default:
            return nil
        }
    }

    var rawValue: Int16 {
        switch self {
        case .incoming(.new): 0
        case .incoming(.accepted): 3
        case .incoming(.declined): 4
        case .incoming(.cancelled): 6
        case .outgoing(.pending): 5
        case .outgoing(.cancelled): 7
        }
    }
}
