import Foundation

extension CharacterSet {
    static var username: CharacterSet {
        CharacterSet(charactersIn: "a" ... "z")
    }
}
