import Foundation
import KeyDerivation
import Products
import Testing

@testable import polkadot_app

@Suite("APSignVrfHandler Tests")
struct APSignVrfHandlerTests {
    let confirmation = MockSignVrfConfirmation()

    func makeHandler(callingProductId: ProductId? = "caller.product") -> APSignVrfHandling {
        APSignVrfHandler(
            callingProductId: callingProductId,
            confirmationRequester: confirmation,
            walletFactory: {
                try DynamicDerivedWallet(
                    derivationPath: $0.derivationPath(),
                    entropyManager: SsoTestData.entropyManager
                )
            },
            logger: MockLogger()
        )
    }

    func makePayload(
        productId: ProductId = "target.product",
        items: [VrfTranscriptItem] = [VrfTranscriptItem(label: Data("key".utf8), value: Data([0x01]))]
    ) -> SignVrfPayload {
        SignVrfPayload(
            account: ProductAccountId(productId: productId, derivationIndex: .index(0)),
            transcriptLabel: Data("test:label".utf8),
            items: items
        )
    }

    @Test("Approved prompt produces a signature and forwards the payload")
    func approvedPromptSigns() async throws {
        let payload = makePayload()

        let signature = try await makeHandler().signVrf(payload)

        #expect(signature.preOutput.count == 32)
        #expect(signature.proof.count == 64)
        #expect(confirmation.requests.count == 1)
        #expect(confirmation.requests.first?.payload == payload)
        #expect(confirmation.requests.first?.callingProductId == "caller.product")
    }

    @Test("Rejected prompt throws rejected and never signs")
    func rejectedPromptThrows() async throws {
        confirmation.decision = .rejected

        await #expect(throws: SignVrfError.rejected) {
            try await makeHandler().signVrf(makePayload())
        }
    }

    // Deliberate difference from create proof, which skips the prompt for same-product
    // callers: sign_vrf is prompt-only on every host.
    @Test("Same-product caller still prompts")
    func sameProductCallerPrompts() async throws {
        let payload = makePayload(productId: "caller.product")

        _ = try await makeHandler(callingProductId: "caller.product").signVrf(payload)

        #expect(confirmation.requests.count == 1)
    }

    @Test("Too many items throw transcriptTooLarge without prompting")
    func tooManyItemsRefusedBeforePrompt() async throws {
        let items = Array(
            repeating: VrfTranscriptItem(label: Data("key".utf8), value: Data([0x01])),
            count: SignVrfPayload.maxTranscriptItems + 1
        )

        await expectTranscriptTooLarge(payload: makePayload(items: items))
    }

    @Test("Oversized transcript throws transcriptTooLarge without prompting")
    func oversizedTranscriptRefusedBeforePrompt() async throws {
        let oversizedValue = Data(repeating: 0, count: SignVrfPayload.maxTranscriptBytes)
        let items = [VrfTranscriptItem(label: Data("key".utf8), value: oversizedValue)]

        await expectTranscriptTooLarge(payload: makePayload(items: items))
    }

    @Test("Transcript at both bounds signs successfully")
    func boundaryTranscriptSigns() async throws {
        let payload = makePayload()
        let itemCount = SignVrfPayload.maxTranscriptItems
        let itemLabel = Data("key".utf8)
        let fixedOverhead = payload.transcriptLabel.count + itemCount * itemLabel.count
        let valueSize = (SignVrfPayload.maxTranscriptBytes - fixedOverhead) / itemCount

        let items = Array(
            repeating: VrfTranscriptItem(label: itemLabel, value: Data(repeating: 0, count: valueSize)),
            count: itemCount
        )

        let signature = try await makeHandler().signVrf(makePayload(items: items))

        #expect(signature.preOutput.count == 32)
        #expect(confirmation.requests.count == 1)
    }

    private func expectTranscriptTooLarge(payload: SignVrfPayload) async {
        do {
            _ = try await makeHandler().signVrf(payload)
            Issue.record("Expected transcriptTooLarge to be thrown")
        } catch let SignVrfError.transcriptTooLarge(reason) {
            #expect(!reason.isEmpty)
        } catch {
            Issue.record("Expected transcriptTooLarge, got \(error)")
        }

        #expect(confirmation.requests.isEmpty)
    }
}
