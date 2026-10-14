import Testing
import Foundation
import HandoffService
import SubstrateSdk
import KeyDerivation
import Keystore_iOS
import NovaCrypto
import SDKLogger
import AsyncExtensions
import CryptoKit
import SubstrateSdkExt
@testable import polkadot_app

struct HandoffServiceTests {
    static let bulletInURL = URL(string: "wss://previewnet.substrate.dev/bulletin")!

    struct MockParticipant {
        let sender: SenderProofProviding
        let recipient: SNKeypairProtocol
    }

    @Test func sendSmallDataP2P() async throws {
        let alice = try MockParticipant.createRandom().sender
        let bob = try MockParticipant.createRandom().recipient

        let dataToSend = try Data.randomOrError(of: 10)

        try await submitBlobAndClaim(dataToSend, sender: alice, recipients: [bob])
    }

    @Test func sendSmallDataToMultiRecipients() async throws {
        let alice = try MockParticipant.createRandom().sender
        let bob = try MockParticipant.createRandom().recipient
        let charlie = try MockParticipant.createRandom().recipient
        let dave = try MockParticipant.createRandom().recipient

        let dataToSend = try Data.randomOrError(of: 10)

        try await submitBlobAndClaim(dataToSend, sender: alice, recipients: [bob, charlie, dave])
    }

    @Test func sendFileToMultiRecipients() async throws {
        let alice = try MockParticipant.createRandom().sender
        let bob = try MockParticipant.createRandom().recipient
        let charlie = try MockParticipant.createRandom().recipient
        let dave = try MockParticipant.createRandom().recipient

        let dataToSend = try Data.randomOrError(of: 4_000_000)

        try await submitFileAndClaim(dataToSend, sender: alice, recipientKeypairs: [bob, charlie, dave])
    }
}

private extension HandoffServiceTests {
    func submitBlobAndClaim(
        _ data: Data,
        sender: SenderProofProviding,
        recipients: [SNKeypairProtocol]
    ) async throws {
        let logger = Logger.shared

        let connection = WebSocketEngine(urls: [Self.bulletInURL], logger: logger)!

        let service = HandoffService(connection: connection)

        let recipientKeys = recipients.map { recipient in
            let pubKey = recipient.publicKey().rawData()
            return MultiSigner.sr25519(pubKey)
        }

        let submittedData = try await service.submitData(
            data,
            from: sender,
            recipients: Set(recipientKeys)
        )

        logger.debug("Submission result: \(submittedData)")

        let dataHash = try data.blake2b32()

        for recipient in recipients {
            let proofProvider = SR25519RecipientProofProvider(signer: SNSigner(keypair: recipient))
            let claimedData = try await service.claimData(
                by: dataHash,
                recipient: proofProvider
            )

            let claimedCount = claimedData?.count ?? 0

            let recipientAddress = try recipient.publicKey().rawData().toAddress(
                using: .genericFormat
            )

            logger.debug("Claimed: \(claimedCount) by: \(recipientAddress)")
        }
    }

    func submitFileAndClaim(
        _ file: Data,
        sender: SenderProofProviding,
        recipientKeypairs: [SNKeypairProtocol]
    ) async throws {
        let logger = Logger.shared

        let connection = WebSocketEngine(urls: [Self.bulletInURL], logger: logger)!

        let service = HandoffService(connection: connection)

        let fileLoader = HandoffFileLoader(service: service)

        let encryptor = ChaChaPolyFileEncryptor(symmetricKey: SymmetricKey(size: .bits256))

        let recipientKeys = recipientKeypairs.map { recipient in
            let pubKey = recipient.publicKey().rawData()
            return MultiSigner.sr25519(pubKey)
        }

        let recipients = FileRecipients(
            pubKeys: Set(recipientKeys),
            encryptor: encryptor
        )

        let store = MockUploadFileContext(fileData: file)
        let uploadStream = fileLoader.uploadFile(
            store: store,
            sender: sender,
            recipients: recipients
        )

        let fileHash = try file.blake2b32()

        for try await uploadEvent in uploadStream {
            switch uploadEvent {
            case let .onProgress(progress):
                logger.debug("Uploaded \(progress.uploaded) out of \(progress.total)")
            case let .onFinished(finished):
                logger.debug("Upload completed")
                try await claimFile(
                    using: fileLoader,
                    entryHash: finished.entryHash,
                    recipientKeypairs: recipientKeypairs,
                    decryptor: encryptor,
                    actualFileHash: fileHash
                )
            case let .onError(error):
                logger.error("Upload failed: \(error)")
            }
        }
    }

    func claimFile(
        using fileLoader: HandoffFileLoader,
        entryHash: Data,
        recipientKeypairs: [SNKeypairProtocol],
        decryptor: FileEncrypting,
        actualFileHash: Data
    ) async throws {
        let logger = Logger.shared

        for recipient in recipientKeypairs {
            let signer = SNSigner(keypair: recipient)

            let store = MockDownloadFileContext(entryHash: entryHash)
            let downloadStream = fileLoader.downloadFile(
                using: entryHash,
                claimer: FileClaimer(
                    proofProvider: SR25519RecipientProofProvider(signer: signer),
                    decryptor: decryptor
                ),
                store: store
            )

            for try await downloadEvent in downloadStream {
                switch downloadEvent {
                case let .onProgress(progress):
                    logger.debug("Downloaded \(progress.downloaded) out of \(progress.total)")
                case .onFinished:
                    let fileData = store.assembleFile()
                    let receivedFileHash = try fileData.blake2b32()

                    let recipientAddress = try recipient.publicKey().rawData().toAddress(
                        using: .genericFormat
                    )

                    logger.debug("Download finished for recipient: \(recipientAddress)")

                    #expect(actualFileHash == receivedFileHash)
                case let .onError(error):
                    logger.error("Download failed: \(error)")
                }
            }
        }
    }
}

extension HandoffServiceTests.MockParticipant {
    static func createRandom() throws -> Self {
        let seed = try Data.randomOrError(of: 32)
        let keypair = try SNKeyFactory().createKeypair(fromSeed: seed)
        let publicKey = keypair.publicKey().rawData()

        let sender = SenderProofProvider(sender: .sr25519(publicKey)) { data in
            let signature = try SNSigner(keypair: keypair).sign(data).rawData()
            return .sr25519(data: signature)
        }

        return HandoffServiceTests.MockParticipant(sender: sender, recipient: keypair)
    }
}
