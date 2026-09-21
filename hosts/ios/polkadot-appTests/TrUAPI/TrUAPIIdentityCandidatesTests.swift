import Foundation
import Testing
import SubstrateSdk
@testable import polkadot_app

struct TrUAPIIdentityCandidatesTests {
    private func response(_ rows: [[String: Any]], cursor: String? = nil) throws -> UsernameSearchResult {
        var payload: [String: Any] = ["usernames": rows]
        payload["nextCursor"] = cursor.map { $0 as Any } ?? NSNull()
        return try JSONDecoder().decode(
            UsernameSearchResult.self,
            from: JSONSerialization.data(withJSONObject: payload)
        )
    }

    private func row(_ username: String, account: String, status: String = "ASSIGNED") -> [String: Any] {
        [
            "username": username,
            "accountId": account,
            "status": status,
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z"
        ]
    }

    @Test
    func nativeSearchShapeSuppliesOnlyExactAccountCandidatesNotStatusAuthority() throws {
        let account = Data(repeating: 3, count: 32)
        let address = try SS58AddressFactory().address(fromAccountId: account, type: 42)
        let result = try response([
            row("alice-other", account: "not-an-address"),
            row("alice", account: address, status: "RESERVED")
        ])
        #expect(try RustHostRuntimeBridge.identityCandidates(from: result, username: "alice") == [account])
        #expect(try RustHostRuntimeBridge.identityCandidates(from: result, username: "ali").isEmpty)
    }

    @Test
    func malformedNativeAccountsAndOversizedSetsCannotClaimResolution() throws {
        let malformed = try response([row("alice", account: "not-an-address")])
        #expect(throws: (any Error).self) {
            try RustHostRuntimeBridge.identityCandidates(from: malformed, username: "alice")
        }
        let address = try SS58AddressFactory().address(fromAccountId: Data(repeating: 3, count: 32), type: 42)
        let oversized = try response(Array(repeating: row("alice", account: address), count: 33))
        #expect(throws: (any Error).self) {
            try RustHostRuntimeBridge.identityCandidates(from: oversized, username: "alice")
        }
        #expect(throws: (any Error).self) {
            try JSONDecoder().decode(UsernameSearchResult.self, from: Data(#"{"accounts":["alice"]}"#.utf8))
        }
    }
}
