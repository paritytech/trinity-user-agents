import Foundation
import UIKit

extension Chat {
    enum PeerAction: Equatable {
        case audioCall
        case videoCall
        case leaveChat
        case blockUser
        case custom(CustomPeerAction)
    }
}

extension Chat {
    struct CustomPeerAction: Equatable {
        private let id = UUID()

        let titleProvider: () -> String
        let image: UIImage?
        let handler: () -> Void

        static func == (lhs: Self, rhs: Self) -> Bool {
            lhs.id == rhs.id
        }
    }
}
