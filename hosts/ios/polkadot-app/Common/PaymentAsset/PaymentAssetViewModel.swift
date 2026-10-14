import Foundation

protocol PaymentAssetViewModelProtocol: AnyObject {
    func bind(_ onBrand: @escaping @MainActor (PaymentAssetBrand) -> Void)
    func cancel()
}

protocol PaymentAssetViewModelMaking {
    func makeViewModel() -> PaymentAssetViewModelProtocol
}

final class PaymentAssetViewModel: PaymentAssetViewModelProtocol {
    private let branding: PaymentAssetBrandingProviding
    private let logger: LoggerProtocol
    private var task: Task<Void, Never>?

    init(branding: PaymentAssetBrandingProviding, logger: LoggerProtocol = Logger.shared) {
        self.branding = branding
        self.logger = logger
    }

    deinit {
        task?.cancel()
    }

    func bind(_ onBrand: @escaping @MainActor (PaymentAssetBrand) -> Void) {
        task?.cancel()
        task = Task { [branding, logger] in
            do {
                for try await brand in branding.stream() {
                    guard !Task.isCancelled else { return }
                    await onBrand(brand)
                }
            } catch {
                logger.error("Payment asset brand stream failed: \(error)")
            }
        }
    }

    func cancel() {
        task?.cancel()
        task = nil
    }
}

struct PaymentAssetViewModelFactory: PaymentAssetViewModelMaking {
    private let branding: PaymentAssetBrandingProviding

    init(branding: PaymentAssetBrandingProviding = PaymentAssetBranding.shared) {
        self.branding = branding
    }

    func makeViewModel() -> PaymentAssetViewModelProtocol {
        PaymentAssetViewModel(branding: branding)
    }
}
