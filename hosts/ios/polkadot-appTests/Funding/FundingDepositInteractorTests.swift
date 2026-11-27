import Testing
import UIKit
@testable import polkadot_app

/// How the deposit screen's interactor draws the deposit's QR code.
@MainActor
struct FundingDepositInteractorTests {
    @Test
    func drawsTheQRCodeForThePayload() async {
        let interactor = FundingDepositInteractor(
            service: FundingService(logger: MockLogger()),
            entryId: "entry",
            logger: MockLogger()
        )
        let output = OutputSpy()
        interactor.depositOutput = output

        let payload = "ethereum:0x0000000000000000000000000000000000000001"
        let received = await withCheckedContinuation { continuation in
            output.onQRCode = { continuation.resume(returning: $0) }
            interactor.createQRCode(for: payload)
        }

        #expect(received.payload == payload)
        #expect(received.image.size.width > 0)
    }
}

@MainActor
private final class OutputSpy: FundingDepositInteractorOutputProtocol {
    var onQRCode: (((payload: String, image: UIImage)) -> Void)?

    func didReceive(qrCode: UIImage, for payload: String) {
        onQRCode?((payload, qrCode))
    }
}
