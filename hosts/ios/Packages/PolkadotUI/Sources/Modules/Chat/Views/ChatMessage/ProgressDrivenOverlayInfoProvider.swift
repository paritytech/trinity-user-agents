import UIKit

public final class ProgressDrivenOverlayInfoProvider: ChatMessageOverlayInfoProviding {
    private let progressViewModel: any LoadingProgressViewModelProtocol
    private let progressConfiguration: (CGFloat) -> ChatMessageOverlayInfoViewConfiguration
    private let successConfiguration: ChatMessageOverlayInfoViewConfiguration?
    private let failureConfiguration: ChatMessageOverlayInfoViewConfiguration?
    private let emitProgressOnSubscription: Bool
    public init(
        progressViewModel: any LoadingProgressViewModelProtocol,
        progressConfiguration: @escaping (CGFloat) -> ChatMessageOverlayInfoViewConfiguration,
        successConfiguration: ChatMessageOverlayInfoViewConfiguration? = nil,
        failureConfiguration: ChatMessageOverlayInfoViewConfiguration? = nil,
        emitProgressOnSubscription: Bool
    ) {
        self.progressViewModel = progressViewModel
        self.progressConfiguration = progressConfiguration
        self.successConfiguration = successConfiguration
        self.failureConfiguration = failureConfiguration
        self.emitProgressOnSubscription = emitProgressOnSubscription
    }

    public func startInfoUpdate(
        onUpdate: @escaping (ChatMessageOverlayInfoViewConfiguration?) -> Void
    ) {
        if emitProgressOnSubscription {
            onUpdate(progressConfiguration(0))
        }
        progressViewModel.startProgressUpdate(
            onProgress: { [weak self] in onUpdate(self?.progressConfiguration($0)) },
            onSuccess: { [weak self] in onUpdate(self?.successConfiguration) },
            onFailure: { [weak self] in onUpdate(self?.failureConfiguration) }
        )
    }

    public func stopInfoUpdate() {
        progressViewModel.stopProgressUpdate()
    }
}

// MARK: - Media factory methods

public extension ProgressDrivenOverlayInfoProvider {
    static func forUpload(
        progressViewModel: any LoadingProgressViewModelProtocol,
        successConfiguration: ChatMessageOverlayInfoViewConfiguration? = nil
    ) -> Self {
        Self(
            progressViewModel: progressViewModel,
            progressConfiguration: { .mediaUploading(progress: $0) },
            successConfiguration: successConfiguration,
            failureConfiguration: .mediaUploadFailed(),
            emitProgressOnSubscription: true
        )
    }

    static func forDownload(
        progressViewModel: any LoadingProgressViewModelProtocol,
        successConfiguration: ChatMessageOverlayInfoViewConfiguration? = nil
    ) -> Self {
        Self(
            progressViewModel: progressViewModel,
            progressConfiguration: { .mediaDownloading(progress: $0) },
            successConfiguration: successConfiguration,
            failureConfiguration: .mediaDownloadFailed(),
            emitProgressOnSubscription: true
        )
    }
}
