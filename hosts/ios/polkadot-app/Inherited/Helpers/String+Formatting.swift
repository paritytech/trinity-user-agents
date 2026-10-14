import Foundation

extension String {
    static func negative(_ suffix: String) -> String {
        "-\(suffix)"
    }

    static func positive(_ suffix: String) -> String {
        "+\(suffix)"
    }

    func trimmingDot() -> String {
        hasSuffix(".dot") ? String(dropLast(4)) : self
    }
}
