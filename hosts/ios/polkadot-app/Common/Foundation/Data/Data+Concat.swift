import Foundation

extension Data {
    func padding(value: UInt8, size: Int) -> Data {
        (self + Data(repeating: value, count: size)).prefix(size)
    }
}
