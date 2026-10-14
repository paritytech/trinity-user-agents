import Foundation

public enum DotNsLoadProgress {
    case idle
    case resolved
    case downloading(fraction: Double)
    case unpacking
    case completed
    case failed(Error)
}
