import SwiftUI

// Mimics collapsed picker
struct DigitsPeekView: View {
    let selection: String?
    let options: [String]
    var rowHeight: CGFloat = 28

    private var selectedIndex: Int? {
        guard let selection else { return nil }
        return options.firstIndex(of: selection)
    }

    private var previousOption: String? {
        guard let index = selectedIndex, index > 0 else { return nil }
        return options[index - 1]
    }

    private var nextOption: String? {
        guard let index = selectedIndex, index < options.count - 1 else { return nil }
        return options[index + 1]
    }

    var body: some View {
        VStack(spacing: 0) {
            row(for: previousOption)
                .opacity(0.3)
            row(for: selection)
            row(for: nextOption)
                .opacity(0.3)
        }
        .frame(height: rowHeight * 2)
        .clipped()
    }

    private func row(for option: String?) -> some View {
        Text(option ?? "")
            .typography(.titleExtraLarge)
            .foregroundStyle(Color(.fgPrimary))
            .frame(maxWidth: .infinity)
            .frame(height: rowHeight)
    }
}

// Expanded picker
struct DigitsPickerView: View {
    @Binding var selection: String?
    let options: [String]
    var onSelectCurrent: (() -> Void)?

    @State private var dragHapticIndex: Int?
    @State private var scrollPosition: String?
    @State private var isScrolling: Bool = false

    private let rowHeight: CGFloat = 28
    private let visibleRows: Int = 7

    private var visibleHeight: CGFloat { rowHeight * CGFloat(visibleRows) }

    var body: some View {
        Group {
            if #available(iOS 18.0, *) {
                modernBody
            } else {
                legacyBody
            }
        }
        .frame(height: visibleHeight)
        .sensoryFeedback(.selection, trigger: dragHapticIndex)
    }

    @available(iOS 18.0, *)
    private var modernBody: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(spacing: 0) {
                ForEach(options, id: \.self) { option in
                    tappableRow(for: option)
                }
            }
            .scrollTargetLayout()
        }
        .frame(height: visibleHeight)
        .contentMargins(.vertical, (visibleHeight - rowHeight) / 2, for: .scrollContent)
        .scrollTargetBehavior(.viewAligned)
        .scrollPosition(id: $scrollPosition, anchor: .center)
        .coordinateSpace(.named("digitsScroll"))
        .onAppear {
            scrollPosition = selection
        }
        .onScrollPhaseChange { _, newPhase in
            isScrolling = newPhase != .idle
            if newPhase == .idle, let position = scrollPosition, position != selection {
                selection = position
            }
        }
        .onChange(of: selection) { _, newValue in
            if scrollPosition != newValue {
                scrollPosition = newValue
            }
        }
        .onChange(of: scrollPosition) { _, newValue in
            if let newValue, let index = options.firstIndex(of: newValue), dragHapticIndex != index {
                dragHapticIndex = index
            }
            if !isScrolling, let newValue, newValue != selection {
                selection = newValue
            }
        }
    }

    private var legacyBody: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical, showsIndicators: false) {
                VStack(spacing: 0) {
                    ForEach(options, id: \.self) { option in
                        tappableRow(for: option)
                    }
                }
                .padding(.vertical, (visibleHeight - rowHeight) / 2)
            }
            .frame(height: visibleHeight)
            .coordinateSpace(name: "digitsScroll")
            .onAppear {
                withAnimation(nil) {
                    proxy.scrollTo(selection, anchor: .center)
                }
            }
            .onChange(of: selection) { _, newValue in
                withAnimation(.easeInOut(duration: 0.2)) {
                    proxy.scrollTo(newValue, anchor: .center)
                }
            }
            .simultaneousGesture(drag(proxy: proxy))
        }
    }

    private func drag(proxy _: ScrollViewProxy) -> some Gesture {
        DragGesture(minimumDistance: 1)
            .onChanged { value in
                guard let newIndex = dragTargetIndex(for: value.translation.height) else { return }
                if newIndex != dragHapticIndex {
                    dragHapticIndex = newIndex
                }
            }
            .onEnded { value in
                guard let newIndex = dragTargetIndex(for: value.translation.height) else { return }
                withAnimation(.easeInOut(duration: 0.2)) {
                    selection = options[newIndex]
                }
            }
    }

    private func dragTargetIndex(for offset: CGFloat) -> Int? {
        guard let selection, let currentIndex = options.firstIndex(of: selection) else { return nil }
        let steps = -Int((offset / rowHeight).rounded())
        return max(0, min(options.count - 1, currentIndex + steps))
    }

    private func tappableRow(for option: String) -> some View {
        row(for: option)
            .id(option)
            .onTapGesture {
                if selection == option {
                    onSelectCurrent?()
                } else {
                    withAnimation(.easeInOut(duration: 0.2)) {
                        selection = option
                        scrollPosition = option
                    }
                }
            }
    }

    private func row(for option: String) -> some View {
        Text(option)
            .typography(.titleExtraLarge)
            .foregroundStyle(Color(.fgPrimary))
            .frame(maxWidth: .infinity)
            .frame(height: rowHeight)
            .visualEffect { [rowHeight, visibleHeight] effect, geo in
                let midY = geo.frame(in: .named("digitsScroll")).midY
                let distance = abs(midY - visibleHeight / 2)
                let normalizedDistance = min(distance / (visibleHeight / 2), 1.0)
                let opacity = distance < rowHeight / 2 ? 1.0 : 1.0 - normalizedDistance * 0.85
                return effect.opacity(opacity)
            }
            .contentShape(Rectangle())
    }
}
