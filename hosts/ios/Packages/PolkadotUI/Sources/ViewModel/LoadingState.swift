import Foundation

public enum LoadingState {
    case loading
    case finished
    case error(Error? = nil)
}
