import Foundation

enum UsernameAvailableType: Hashable {
    case available(digits: [Int])
    case taken
    case invalid
    case error(String)
}
