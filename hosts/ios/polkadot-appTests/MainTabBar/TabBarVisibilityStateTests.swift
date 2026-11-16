import Foundation
import PolkadotUI
import Testing
@testable import polkadot_app

@Suite("TabBarVisibilityState")
struct TabBarVisibilityStateTests {
    @Test("A tucked override tucks the bar")
    func tuckedOverrideTucks() {
        let state = TabBarVisibilityState(derived: .none, override: .tucked)
        #expect(state == .tucked)
    }

    @Test("With no override and nothing derived the bar is shown")
    func noOverrideNoDerivedIsShown() {
        let state = TabBarVisibilityState(derived: .none, override: .none)
        #expect(state == .shown)
    }

    @Test("A derived tuck applies when no override is set")
    func derivedTuckWithoutOverrideIsTucked() {
        let state = TabBarVisibilityState(derived: .tucked, override: .none)
        #expect(state == .tucked)
    }

    @Test("A derived hide applies when no override is set")
    func derivedHideWithoutOverrideIsHidden() {
        let state = TabBarVisibilityState(derived: .hidden, override: .none)
        #expect(state == .hidden)
    }

    @Test("A derived hide wins over a tucked override")
    func derivedHideBeatsTuckedOverride() {
        let state = TabBarVisibilityState(derived: .hidden, override: .tucked)
        #expect(state == .hidden)
    }

    @Test("Only a tucked bar carries its tuck to the next screen")
    func onlyTuckedBarCarriesOverride() {
        #expect(TabBarVisibilityState.tucked.carriedOverride == .tucked)
        #expect(TabBarVisibilityState.shown.carriedOverride == .none)
        #expect(TabBarVisibilityState.hidden.carriedOverride == .none)
    }

    @Test("A tucked bar stays tucked on a normal screen")
    func tuckedCarriesToNormalScreen() {
        let carried = TabBarVisibilityState.tucked.carriedOverride
        #expect(TabBarVisibilityState(derived: .none, override: carried) == .tucked)
    }

    @Test("A shown bar shows on a root")
    func shownCarriesToRootAsShown() {
        let carried = TabBarVisibilityState.shown.carriedOverride
        #expect(TabBarVisibilityState(derived: .none, override: carried) == .shown)
    }

    @Test("Leaving a hidden chat for a root shows the bar")
    func hiddenToRootIsShown() {
        let carried = TabBarVisibilityState.hidden.carriedOverride
        #expect(TabBarVisibilityState(derived: .none, override: carried) == .shown)
    }

    @Test("A shown bar tucks on a mounted product")
    func shownToMountedProductIsTucked() {
        let carried = TabBarVisibilityState.shown.carriedOverride
        #expect(TabBarVisibilityState(derived: .tucked, override: carried) == .tucked)
    }

    @Test("Only a swap between tucked and hidden is written without animation")
    func offScreenSwapIsTuckedAndHiddenOnly() {
        #expect(TabBarVisibilityState.tucked.isOffScreenSwap(to: .hidden))
        #expect(TabBarVisibilityState.hidden.isOffScreenSwap(to: .tucked))
        #expect(!TabBarVisibilityState.shown.isOffScreenSwap(to: .hidden))
        #expect(!TabBarVisibilityState.hidden.isOffScreenSwap(to: .shown))
        #expect(!TabBarVisibilityState.shown.isOffScreenSwap(to: .tucked))
    }

    @Test("A shown bar publishes its resting height above the safe area as clearance")
    func shownClearanceIsRestingHeightAboveSafeArea() {
        let clearance = TabBarVisibilityState.shown.bottomClearance(bottomSafeAreaInset: 34)
        let expected: CGFloat = DSTabBarStyle.restingHeight(bottomSafeAreaInset: 34) - 34
        #expect(clearance == expected)
    }

    @Test("A tucked bar publishes no clearance")
    func tuckedClearanceIsZero() {
        #expect(TabBarVisibilityState.tucked.bottomClearance(bottomSafeAreaInset: 34) == 0)
    }

    @Test("A hidden bar publishes no clearance")
    func hiddenClearanceIsZero() {
        #expect(TabBarVisibilityState.hidden.bottomClearance(bottomSafeAreaInset: 34) == 0)
    }
}
