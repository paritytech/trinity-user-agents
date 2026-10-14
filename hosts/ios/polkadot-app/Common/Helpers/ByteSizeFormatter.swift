import Foundation

enum ByteSizeFormatter {
    static func string(fromBytes bytes: Int) -> String {
        let formatter = ByteCountFormatter()
        formatter.countStyle = .binary
        formatter.allowedUnits = [.useGB, .useMB, .useKB, .useBytes]
        return formatter.string(fromByteCount: Int64(bytes))
    }
}
