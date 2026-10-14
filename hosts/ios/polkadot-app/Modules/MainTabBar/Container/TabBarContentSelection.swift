import Foundation

enum TabBarContentSelection: Equatable {
    case tab(Int)
    case spa(UUID)

    var isSPA: Bool {
        if case .spa = self { return true }
        return false
    }
}
