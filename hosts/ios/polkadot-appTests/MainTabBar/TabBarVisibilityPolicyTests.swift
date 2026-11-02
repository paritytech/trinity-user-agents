import Testing
@testable import polkadot_app

@Suite("TabBarVisibilityPolicy")
struct TabBarVisibilityPolicyTests {
    @Test("A tucked override tucks the bar")
    func tuckedOverrideTucks() {
        let state = TabBarVisibilityPolicy.state(derived: .none, override: .tucked)
        #expect(state == .tucked)
    }

    @Test("With no override and nothing derived the bar is shown")
    func noOverrideNoDerivedIsShown() {
        let state = TabBarVisibilityPolicy.state(derived: .none, override: .none)
        #expect(state == .shown)
    }

    @Test("A derived tuck applies when no override is set")
    func derivedTuckWithoutOverrideIsTucked() {
        let state = TabBarVisibilityPolicy.state(derived: .tucked, override: .none)
        #expect(state == .tucked)
    }

    @Test("A derived hide applies when no override is set")
    func derivedHideWithoutOverrideIsHidden() {
        let state = TabBarVisibilityPolicy.state(derived: .hidden, override: .none)
        #expect(state == .hidden)
    }
}
