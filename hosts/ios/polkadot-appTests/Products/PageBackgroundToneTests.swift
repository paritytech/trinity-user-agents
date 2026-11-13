import Testing
@testable import polkadot_app

struct PageBackgroundToneTests {
    @Test func darkBodyOverTransparentRootReadsDark() {
        #expect(PageBackgroundTone(cssColors: ["rgba(0, 0, 0, 0)", "rgb(5, 5, 5)"]) == .dark)
    }

    @Test func lightRootReadsLight() {
        #expect(PageBackgroundTone(cssColors: ["rgb(247, 247, 248)", "rgb(247, 247, 248)"]) == .light)
    }

    @Test func outermostVisibleColourWins() {
        #expect(PageBackgroundTone(cssColors: ["rgb(250, 250, 250)", "rgb(0, 0, 0)"]) == .light)
    }

    @Test func translucentColourCounts() {
        #expect(PageBackgroundTone(cssColors: ["rgba(10, 10, 10, 0.5)"]) == .dark)
    }

    @Test(arguments: [
        [String](),
        ["rgba(0, 0, 0, 0)", "rgba(0, 0, 0, 0)"],
        ["transparent"],
        ["color(srgb 0 0 0)"],
        ["rgb(1, 2)"]
    ])
    func pageWithoutVisibleBackgroundHasNoTone(cssColors: [String]) {
        #expect(PageBackgroundTone(cssColors: cssColors) == nil)
    }

    @Test func darkPageUnderLightThemeTakesNightStrip() {
        #expect(PageBackgroundTone.dark.statusStripTheme(appThemeIsLight: true) == .berlinNight)
    }

    @Test func lightPageUnderDarkThemeTakesDayStrip() {
        #expect(PageBackgroundTone.light.statusStripTheme(appThemeIsLight: false) == .berlinDay)
    }

    @Test func pageMatchingTheAppThemeKeepsItsStrip() {
        #expect(PageBackgroundTone.dark.statusStripTheme(appThemeIsLight: false) == nil)
        #expect(PageBackgroundTone.light.statusStripTheme(appThemeIsLight: true) == nil)
    }
}
