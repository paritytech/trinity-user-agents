import struct TrUAPIHost.ProductId
import Foundation
import Testing
@testable import Products

struct ProductManifestRecordsTests {
    @Test(arguments: ExecutableKind.allCases)
    func buildsSubnameForEachKind(_ kind: ExecutableKind) throws {
        let subname = try ProductManifestRecords.subname(base: ProductId("hackm3.dot"), kind: kind)

        #expect(subname.rawValue == "\(kind.rawValue).hackm3.dot")
        #expect(try ProductManifestRecords.baseName(of: subname).rawValue == "hackm3.dot")
    }

    @Test func stripsKindPrefixToRecoverTheBaseName() throws {
        #expect(try ProductManifestRecords.baseName(of: ProductId("worker.hackm3.dot")).rawValue == "hackm3.dot")
        #expect(try ProductManifestRecords.baseName(of: ProductId("hackm3.dot")).rawValue == "hackm3.dot")
    }

    /// `app.<tld>` is a product named after a kind, not the `app` subname of the bare TLD.
    @Test func doesNotStripPrefixWhenNothingDottedRemains() throws {
        #expect(try ProductManifestRecords.baseName(of: ProductId("app.dot")).rawValue == "app.dot")
        #expect(try ProductManifestRecords.baseName(of: ProductId("worker.paseo")).rawValue == "worker.paseo")
    }

    @Test func leavesUnrelatedPrefixesAlone() throws {
        #expect(try ProductManifestRecords.baseName(of: ProductId("application.dot")).rawValue == "application.dot")
    }

    /// The subname convention is about the kind label, never the suffix, so it has to hold on
    /// whatever TLD the network resolves.
    @Test(arguments: ["dot", "paseo", "testnet"])
    func appliesTheSameConventionUnderAnyTld(_ tld: String) throws {
        let base = try ProductId("hackm3.\(tld)")
        let subname = try ProductManifestRecords.subname(base: base, kind: .worker)

        #expect(subname.rawValue == "worker.hackm3.\(tld)")
        #expect(try ProductManifestRecords.baseName(of: subname) == base)
    }
    @Test func refusesAnExecutableWhoseBaseIsReservedForLocalDevelopment() throws {
        let executable = try ProductId("app.localhost.dot")

        #expect(throws: (any Error).self) {
            try ProductManifestRecords.baseName(of: executable)
        }
    }

}
