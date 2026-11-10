import UIKit
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
import KeyDerivation
import ChainRegistry
import SubstrateSdkExt

final class IdentityDetailsInteractor {
    weak var presenter: IdentityDetailsInteractorOutputProtocol?

    let shareFactory: AccountShareFactoryProtocol
    let chain: ChainModel
    let wallet: WalletManaging
    let qrEncoder: AddressQREncodable

    private let profileService: IdentityProfileServiceProtocol
    private let qrCodeCreationOperationFactory: QRCreationOperationFactoryProtocol
    private let logger: LoggerProtocol

    private var qrTask: Task<Void, Never>?
    private var profileTask: Task<Void, Never>?

    init(
        shareFactory: AccountShareFactoryProtocol,
        profileService: IdentityProfileServiceProtocol,
        chain: ChainModel,
        wallet: WalletManaging,
        qrEncoder: AddressQREncodable,
        qrCodeCreationOperationFactory: QRCreationOperationFactoryProtocol,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.shareFactory = shareFactory
        self.profileService = profileService
        self.chain = chain
        self.wallet = wallet
        self.qrEncoder = qrEncoder
        self.qrCodeCreationOperationFactory = qrCodeCreationOperationFactory
        self.logger = logger
    }

    deinit {
        profileTask?.cancel()
        qrTask?.cancel()
    }
}

extension IdentityDetailsInteractor: IdentityDetailsInteractorInputProtocol {
    func setup() {
        subscribeToProfile()
    }

    func shareMessage(username: Username, image: UIImage) -> [Any] {
        shareFactory.createSources(username: username, qrImage: image)
    }

    func generateQrCode(for size: CGSize) {
        qrTask?.cancel()

        guard
            let address = try? wallet.getRawPublicKey().toAddress(using: chain.chainFormat),
            let payload = try? qrEncoder.encode(address: address)
        else {
            assertionFailure()
            return
        }

        let wrapper = qrCodeCreationOperationFactory.createOperation(
            payload: payload,
            qrSize: size
        )

        qrTask = Task { [logger, weak presenter] in
            do {
                let qrImage = try await wrapper.asyncExecute()
                try Task.checkCancellation()
                await presenter?.didReceive(qrCode: qrImage)
            } catch is CancellationError {
                return
            } catch {
                logger.error("QR generation failed: \(error)")
            }
        }
    }
}

private extension IdentityDetailsInteractor {
    func subscribeToProfile() {
        profileTask = Task { [profileService, logger, weak presenter] in
            do {
                for try await profile in profileService.observe() {
                    await presenter?.didReceive(profile: profile)
                }
            } catch {
                logger.error("IdentityProfile subscription failed: \(error)")
            }
        }
    }
}
