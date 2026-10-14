import Foundation

struct SecretPhraseMnemonicViewModel {
    enum Section: Hashable {
        case main
    }

    enum Cell: Hashable {
        case phrase(Int, String)

        var text: String {
            switch self {
            case let .phrase(_, text): text
            }
        }

        var index: Int {
            switch self {
            case let .phrase(index, _):
                index
            }
        }
    }

    let cells: [Cell]
}
