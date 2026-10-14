import Foundation
import KeyDerivation
import SubstrateSdk
import Testing

@Suite("Sr25519VrfSigner Tests")
struct Sr25519VrfSignerTests {
    let wallet: DynamicDerivedWallet

    init() throws {
        wallet = try DynamicDerivedWallet(seedBytes: Data.randomOrError(of: 32))
    }

    @Test("Signature has 32-byte pre-output and 64-byte proof")
    func signatureSizes() throws {
        let signature = try Sr25519VrfSigner.sign(
            wallet: wallet,
            transcriptLabel: Data("test:label".utf8),
            items: [VrfTranscriptItem(label: Data("key".utf8), value: Data.randomOrError(of: 16))]
        )

        #expect(signature.preOutput.count == 32)
        #expect(signature.proof.count == 64)
    }

    @Test("Pre-output is deterministic for a fixed wallet and transcript")
    func preOutputIsDeterministic() throws {
        let items = try [VrfTranscriptItem(label: Data("key".utf8), value: Data.randomOrError(of: 16))]
        let label = Data("test:label".utf8)

        let first = try Sr25519VrfSigner.sign(wallet: wallet, transcriptLabel: label, items: items)
        let second = try Sr25519VrfSigner.sign(wallet: wallet, transcriptLabel: label, items: items)

        #expect(first.preOutput == second.preOutput)
    }

    @Test("Different transcripts produce different pre-outputs")
    func differentTranscriptsDiffer() throws {
        let label = Data("test:label".utf8)
        let first = try Sr25519VrfSigner.sign(
            wallet: wallet,
            transcriptLabel: label,
            items: [VrfTranscriptItem(label: Data("key".utf8), value: Data([0x01]))]
        )
        let second = try Sr25519VrfSigner.sign(
            wallet: wallet,
            transcriptLabel: label,
            items: [VrfTranscriptItem(label: Data("key".utf8), value: Data([0x02]))]
        )

        #expect(first.preOutput != second.preOutput)
    }
}
