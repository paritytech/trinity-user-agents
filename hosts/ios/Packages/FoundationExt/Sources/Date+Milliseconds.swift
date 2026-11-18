import Foundation

public extension Date {
    init(milliseconds: UInt64) {
        self.init(timeIntervalSince1970: milliseconds.millisecondsToSeconds())
    }
}
