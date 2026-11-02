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

    @Test("A derived hide wins over a tucked override")
    func derivedHideBeatsTuckedOverride() {
        let state = TabBarVisibilityPolicy.state(derived: .hidden, override: .tucked)
        #expect(state == .hidden)
    }

    @Test("Only a tucked bar carries its tuck to the next screen")
    func onlyTuckedBarCarriesOverride() {
        #expect(TabBarVisibilityPolicy.carriedOverride(from: .tucked) == .tucked)
        #expect(TabBarVisibilityPolicy.carriedOverride(from: .shown) == .none)
        #expect(TabBarVisibilityPolicy.carriedOverride(from: .hidden) == .none)
    }

    @Test("A tucked bar stays tucked on a normal screen")
    func tuckedCarriesToNormalScreen() {
        let carried = TabBarVisibilityPolicy.carriedOverride(from: .tucked)
        #expect(TabBarVisibilityPolicy.state(derived: .none, override: carried) == .tucked)
    }

    @Test("A shown bar shows on a root")
    func shownCarriesToRootAsShown() {
        let carried = TabBarVisibilityPolicy.carriedOverride(from: .shown)
        #expect(TabBarVisibilityPolicy.state(derived: .none, override: carried) == .shown)
    }

    @Test("Leaving a hidden chat for a root shows the bar")
    func hiddenToRootIsShown() {
        let carried = TabBarVisibilityPolicy.carriedOverride(from: .hidden)
        #expect(TabBarVisibilityPolicy.state(derived: .none, override: carried) == .shown)
    }

    @Test("A shown bar tucks on a mounted product")
    func shownToMountedProductIsTucked() {
        let carried = TabBarVisibilityPolicy.carriedOverride(from: .shown)
        #expect(TabBarVisibilityPolicy.state(derived: .tucked, override: carried) == .tucked)
    }

    @Test("Only a swap between tucked and hidden is written without animation")
    func offScreenSwapIsTuckedAndHiddenOnly() {
        #expect(TabBarVisibilityPolicy.isOffScreenSwap(from: .tucked, to: .hidden))
        #expect(TabBarVisibilityPolicy.isOffScreenSwap(from: .hidden, to: .tucked))
        #expect(!TabBarVisibilityPolicy.isOffScreenSwap(from: .shown, to: .hidden))
        #expect(!TabBarVisibilityPolicy.isOffScreenSwap(from: .hidden, to: .shown))
        #expect(!TabBarVisibilityPolicy.isOffScreenSwap(from: .shown, to: .tucked))
    }
}
