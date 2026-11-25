import Foundation
import Operation_iOS
import UIKit

final class FundingDepositInteractor: FundingEntryInteractor, FundingDepositInteractorInputProtocol {
    weak var depositOutput: FundingDepositInteractorOutputProtocol?

    private let qrFactory: QRCreationOperationFactoryProtocol
    private var qrCodeTask: Task<Void, Never>?

    init(
        service: FundingServiceProtocol,
        entryId: String,
        qrFactory: QRCreationOperationFactoryProtocol = QRCreationOperationFactory(chainStyle: nil),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.qrFactory = qrFactory
        super.init(service: service, entryId: entryId, logger: logger)
    }

    deinit {
        qrCodeTask?.cancel()
    }

    func stopStarting() {
        service.stopStarting(entryId)
    }

    func createQRCode(for payload: String) {
        qrCodeTask?.cancel()

        let wrapper = qrFactory.createOperation(payload: Data(payload.utf8), qrSize: Constants.qrSize)
        qrCodeTask = Task { [weak self, entryId, logger] in
            do {
                let image = try await wrapper.asyncExecute()
                guard !Task.isCancelled else { return }
                self?.depositOutput?.didReceive(qrCode: image, for: payload)
            } catch {
                if !Task.isCancelled {
                    logger.error("[funding] \(entryId): drawing the deposit QR code failed: \(error)")
                }
            }
        }
    }
}

private extension FundingDepositInteractor {
    enum Constants {
        static let qrSize = CGSize(width: 220, height: 220)
    }
}
