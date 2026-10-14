import Foundation
import Operation_iOS

extension Array where Element: Identifiable {
    func reduceToDict() -> [String: Element] {
        reduce(into: [:]) { $0[$1.identifier] = $1 }
    }
}
