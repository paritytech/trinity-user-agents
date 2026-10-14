import Foundation

public enum LoadingDirection {
    case upload
    case download
}

public protocol LoadingProgressViewModelProtocol: AnyObject {
    var loadingDirection: LoadingDirection { get }

    func startProgressUpdate(
        onProgress: @escaping (CGFloat) -> Void,
        onSuccess: @escaping () -> Void,
        onFailure: @escaping () -> Void
    )

    func stopProgressUpdate()
}
