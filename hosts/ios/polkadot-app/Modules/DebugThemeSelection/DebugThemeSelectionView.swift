import DesignSystem
import SwiftUI

struct DebugThemeSelectionView: View {
    private let themeManager: ThemeManagerProtocol
    private let typographyManager: TypographyManagerProtocol

    @State private var mode: ThemeMode
    @State private var typography: TypographySelection

    init(
        themeManager: ThemeManagerProtocol,
        typographyManager: TypographyManagerProtocol
    ) {
        self.themeManager = themeManager
        self.typographyManager = typographyManager
        _mode = State(initialValue: themeManager.mode)
        _typography = State(initialValue: typographyManager.selection)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                themeModeSection
                typographyFamilySection
                colorsSection
                typographySection
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(.bgSurfaceMain)
    }

    // MARK: Sections

    private var themeModeSection: some View {
        section(title: "Theme Mode") {
            VStack(spacing: 0) {
                let selections = Array(ThemeSelection.allCases.enumerated())
                ForEach(selections, id: \.element) { index, selection in
                    let optionMode: ThemeMode = .app(selection)
                    Button {
                        select(optionMode)
                    } label: {
                        HStack(spacing: 8) {
                            Text(ThemesRegistry.makeTheme(selection).id)
                                .typography(.bodyLarge)
                                .foregroundStyle(.fgPrimary)
                            Spacer()
                            if optionMode == mode {
                                Image(systemName: "checkmark")
                                    .foregroundStyle(.fgSuccess)
                            }
                        }
                        .padding(.vertical, 12)
                        .padding(.horizontal, 16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    .contentShape(Rectangle())

                    if index < selections.count - 1 {
                        separator
                    }
                }
            }
            .container
        }
    }

    private var typographyFamilySection: some View {
        section(title: "Typography Family") {
            VStack(spacing: 0) {
                let selections = Array(TypographySelection.allCases.enumerated())
                ForEach(selections, id: \.element) { index, selection in
                    Button {
                        selectTypography(selection)
                    } label: {
                        HStack(spacing: 8) {
                            Text(TypographyFamiliesRegistry.makeFamily(selection).id)
                                .typography(.bodyLarge)
                                .foregroundStyle(.fgPrimary)
                            Spacer()
                            if selection == typography {
                                Image(systemName: "checkmark")
                                    .foregroundStyle(.fgSuccess)
                            }
                        }
                        .padding(.vertical, 12)
                        .padding(.horizontal, 16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    .contentShape(Rectangle())

                    if index < selections.count - 1 {
                        separator
                    }
                }
            }
            .container
        }
    }

    private var colorsSection: some View {
        section(title: "Colors") {
            VStack(spacing: 0) {
                colorRow("fg.primary", Color.fgPrimary, isFirst: true)
                separator
                colorRow("fg.secondary", Color.fgSecondary)
                separator
                colorRow("fg.success", Color.fgSuccess)
                separator
                colorRow("fg.error", Color.fgError)
                separator
                colorRow("bg.surface.main", Color.bgSurfaceMain)
                separator
                colorRow("bg.surface.container", Color.bgSurfaceContainer)
                separator
                colorRow("bg.surface.nested", Color.bgSurfaceNested)
                separator
                colorRow("bg.accent", Color.bgAccent)
                separator
                colorRow("stroke.primary", Color.strokePrimary, isLast: true)
            }
            .container
        }
    }

    private var typographySection: some View {
        section(title: "Typography") {
            VStack(alignment: .leading, spacing: 12) {
                Text(verbatim: "Display Large").typography(.displayLarge)
                Text(verbatim: "Headline Medium").typography(.headlineMedium)
                Text(verbatim: "Title Large").typography(.titleLarge)
                Text(verbatim: "Body Medium · regular").typography(.bodyMedium)
                Text(verbatim: "Body Medium · emphasized").typography(.bodyMedium.emphasized)
                Text(verbatim: "Paragraph Medium · mono").typography(.paragraphMedium.mono)
                Text(verbatim: "Label Small").typography(.labelSmall)
            }
            .foregroundStyle(.fgPrimary)
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .container
        }
    }

    // MARK: Pieces

    @ViewBuilder
    private func section(title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title.uppercased())
                .typography(.labelSmall)
                .foregroundStyle(.fgSecondary)
                .padding(.horizontal, 4)
            content()
        }
    }

    private var separator: some View {
        Rectangle()
            .fill(Color.strokePrimary)
            .frame(height: 1)
    }

    private func colorRow(
        _ label: String,
        _ color: Color,
        isFirst _: Bool = false,
        isLast _: Bool = false
    ) -> some View {
        HStack(spacing: 12) {
            RoundedRectangle(cornerRadius: DSRadii.tiny)
                .fill(color)
                .frame(width: 28, height: 28)
                .overlay(
                    RoundedRectangle(cornerRadius: DSRadii.tiny)
                        .stroke(Color.strokePrimary, lineWidth: 1)
                )
            Text(label)
                .typography(.bodyMedium.mono)
                .foregroundStyle(.fgPrimary)
            Spacer()
        }
        .padding(.vertical, 8)
        .padding(.horizontal, 16)
    }

    // MARK: Actions

    private func select(_ newMode: ThemeMode) {
        themeManager.select(newMode)
        mode = newMode
    }

    private func selectTypography(_ newSelection: TypographySelection) {
        typographyManager.select(newSelection)
        typography = newSelection
    }
}

private extension View {
    /// Themed container chrome shared by every section block.
    var container: some View {
        background(.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: DSRadii.medium))
            .overlay(
                RoundedRectangle(cornerRadius: DSRadii.medium)
                    .stroke(Color.strokePrimary, lineWidth: 1)
            )
    }
}
