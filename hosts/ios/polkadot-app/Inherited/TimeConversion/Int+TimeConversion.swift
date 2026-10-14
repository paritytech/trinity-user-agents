import Foundation

extension Int {
    func secondsFromDays() -> TimeInterval {
        UInt64(self).secondsFromDays()
    }
}
