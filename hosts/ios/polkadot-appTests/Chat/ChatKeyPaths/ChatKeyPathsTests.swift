import Foundation
import Testing
@testable import polkadot_app

struct ChatKeyPathsTests {
    private let peerAccountId = Data(repeating: 0xAB, count: 32)
    private let peerHex = "0x" + String(repeating: "ab", count: 32)

    @Test func statementAccountLivesUnderChatProduct() {
        #expect(
            ChatKeyPaths.statement(tld: "dot", peerAccountId: peerAccountId) ==
                "//product//chat.dot//peer-statement//" + peerHex
        )
    }

    @Test func hopAccountIsSharedUnderChatProduct() {
        #expect(ChatKeyPaths.hop(tld: "dot") == "//product//chat.dot//hop")
    }

    @Test func requestAccountIsOneShotPerPeriod() throws {
        #expect(try ChatKeyPaths.request(requestId: "request-1", period: 20_371) == "//chat-request//request-1//20371")
    }

    @Test func acceptanceAccountIsOneShotPerPeriod() throws {
        #expect(try ChatKeyPaths.accept(requestId: "request-1", period: 20_371) == "//chat-accept//request-1//20371")
    }

    @Test func refusesRequestIdThatInjectsJunctions() {
        #expect(throws: ChatKeyPathsError.invalidJunction("a//b")) {
            try ChatKeyPaths.request(requestId: "a//b", period: 1)
        }
        #expect(throws: ChatKeyPathsError.invalidJunction("a//b")) {
            try ChatKeyPaths.accept(requestId: "a//b", period: 1)
        }
    }

    @Test func derivesDistinctAccountsPerRoleAndPeer() throws {
        let signManager = ChatSignerManager(entropyManager: FixedRootEntropyManager())
        let otherPeer = Data(repeating: 0xCD, count: 32)

        let paths = try [
            WalletDerivationPath.main(for: "dot"),
            ChatKeyPaths.statement(tld: "dot", peerAccountId: peerAccountId),
            ChatKeyPaths.hop(tld: "dot"),
            ChatKeyPaths.statement(tld: "dot", peerAccountId: otherPeer),
            ChatKeyPaths.statement(tld: "paseo", peerAccountId: peerAccountId),
            ChatKeyPaths.request(requestId: "request-1", period: 1),
            ChatKeyPaths.accept(requestId: "request-1", period: 1),
            ChatKeyPaths.accept(requestId: "request-1", period: 2)
        ]

        let accounts = try paths.map { try signManager.makeSigner(for: $0).accountId }

        #expect(Set(accounts).count == paths.count)
    }

    @Test func restoreFromSameMnemonicReproducesAccounts() throws {
        let path = ChatKeyPaths.statement(tld: "dot", peerAccountId: peerAccountId)

        let original = try ChatSignerManager(entropyManager: FixedRootEntropyManager()).makeSigner(for: path)
        let restored = try ChatSignerManager(entropyManager: FixedRootEntropyManager()).makeSigner(for: path)

        #expect(original.accountId == restored.accountId)
    }
}
