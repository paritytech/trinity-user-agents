import UIKit

public final class ProgressDrivenButtonConfigurationProvider: ChatMessageMediaButtonConfigurationProviding {
    private let progressViewModel: any LoadingProgressViewModelProtocol
    private let successConfiguration: ChatMessageMediaViewConfiguration.ButtonConfiguration?
    private let failureConfiguration: ChatMessageMediaViewConfiguration.ButtonConfiguration?
    private let loadingConfiguration: ChatMessageMediaViewConfiguration.ButtonConfiguration?
    private let emitProgressOnSubscription: Bool
    public init(
        progressViewModel: any LoadingProgressViewModelProtocol,
        successConfiguration: ChatMessageMediaViewConfiguration.ButtonConfiguration?,
        failureConfiguration: ChatMessageMediaViewConfiguration.ButtonConfiguration?,
        loadingConfiguration: ChatMessageMediaViewConfiguration.ButtonConfiguration? = .init(
            style: .loading(cancelable: false)
        ),
        emitProgressOnSubscription: Bool
    ) {
        self.progressViewModel = progressViewModel
        self.successConfiguration = successConfiguration
        self.failureConfiguration = failureConfiguration
        self.loadingConfiguration = loadingConfiguration
        self.emitProgressOnSubscription = emitProgressOnSubscription
    }

    public func startUpdate(
        onUpdate: @escaping (ChatMessageMediaViewConfiguration.ButtonConfiguration?) -> Void
    ) {
        if emitProgressOnSubscription {
            onUpdate(loadingConfiguration)
        }

        progressViewModel.startProgressUpdate(
            onProgress: { [weak self] _ in onUpdate(self?.loadingConfiguration) },
            onSuccess: { [weak self] in onUpdate(self?.successConfiguration) },
            onFailure: { [weak self] in onUpdate(self?.failureConfiguration) }
        )
    }

    public func stopUpdate() {
        progressViewModel.stopProgressUpdate()
    }
}
