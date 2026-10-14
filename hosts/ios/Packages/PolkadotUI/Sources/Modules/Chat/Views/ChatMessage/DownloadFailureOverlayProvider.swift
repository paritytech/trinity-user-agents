import Foundation

public final class DownloadFailureOverlayProvider: ChatMessageMediaFailureOverlayProviding {
    private let progressViewModel: any LoadingProgressViewModelProtocol

    public init(progressViewModel: any LoadingProgressViewModelProtocol) {
        self.progressViewModel = progressViewModel
    }

    public func startUpdate(onUpdate: @escaping (_ isFailureVisible: Bool) -> Void) {
        progressViewModel.startProgressUpdate(
            onProgress: { _ in onUpdate(false) },
            onSuccess: { onUpdate(false) },
            onFailure: { onUpdate(true) }
        )
    }

    public func stopUpdate() {
        progressViewModel.stopProgressUpdate()
    }
}
