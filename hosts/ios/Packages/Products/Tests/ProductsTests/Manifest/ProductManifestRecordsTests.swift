import Foundation
import Testing
@testable import Products

struct ProductManifestRecordsTests {
    @Test(arguments: ExecutableKind.allCases)
    func buildsSubnameForEachKind(_ kind: ExecutableKind) {
        let subname = ProductManifestRecords.subname(base: "hackm3.dot", kind: kind)

        #expect(subname == "\(kind.rawValue).hackm3.dot")
        #expect(ProductManifestRecords.baseName(of: subname) == "hackm3.dot")
    }

    @Test func stripsKindPrefixToRecoverTheBaseName() {
        #expect(ProductManifestRecords.baseName(of: "worker.hackm3.dot") == "hackm3.dot")
        #expect(ProductManifestRecords.baseName(of: "hackm3.dot") == "hackm3.dot")
    }

    /// `app.<tld>` is a product named after a kind, not the `app` subname of the bare TLD.
    @Test func doesNotStripPrefixWhenNothingDottedRemains() {
        #expect(ProductManifestRecords.baseName(of: "app.dot") == "app.dot")
        #expect(ProductManifestRecords.baseName(of: "worker.paseo") == "worker.paseo")
    }

    @Test func leavesUnrelatedPrefixesAlone() {
        #expect(ProductManifestRecords.baseName(of: "application.dot") == "application.dot")
    }

    /// The subname convention is about the kind label, never the suffix, so it has to hold on
    /// whatever TLD the network resolves.
    @Test(arguments: ["dot", "paseo", "test"])
    func appliesTheSameConventionUnderAnyTld(_ tld: String) {
        let base = "hackm3.\(tld)"
        let subname = ProductManifestRecords.subname(base: base, kind: .worker)

        #expect(subname == "worker.hackm3.\(tld)")
        #expect(ProductManifestRecords.baseName(of: subname) == base)
    }
}
