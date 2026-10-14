import DesignSystem
import SwiftUI
import UIKit

@MainActor
struct IssueReportViewLayout: View {
    @Bindable var viewModel: IssueReportViewModel
    let screenshot: UIImage
    @Environment(\.dismiss) private var dismiss
    @FocusState private var isEditing: Bool

    var body: some View {
        VStack(spacing: 24) {
            header
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Text(String(localized: .reportIssueQuestion))
                        .typography(.titleMedium.emphasized)
                    descriptionField
                    Text(String(localized: .reportIssueAttachments))
                        .typography(.bodyMedium)
                        .foregroundStyle(.fgSecondary)
                    Image(uiImage: screenshot)
                        .resizable()
                        .scaledToFit()
                        .frame(height: 220)
                        .clipShape(RoundedRectangle(cornerRadius: 16))
                        .accessibilityLabel(String(localized: .reportIssueScreenshot))
                    if let errorMessage = viewModel.errorMessage {
                        Text(errorMessage)
                            .typography(.bodyMedium)
                            .foregroundStyle(.fgError)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollDismissesKeyboard(.interactively)
            sendButton
        }
        .padding(24)
        .foregroundStyle(.fgPrimary)
        .background(.bgSurfaceMain)
        .interactiveDismissDisabled(viewModel.isSending)
        .onDisappear { viewModel.cancel() }
        .alert(String(localized: .reportIssueThanks), isPresented: $viewModel.isSent) {
            Button(String(localized: .reportIssueDone)) { dismiss() }
        }
    }
}

@MainActor
private extension IssueReportViewLayout {
    var header: some View {
        Text(String(localized: .reportIssueTitle))
            .typography(.titleLarge.emphasized)
            .frame(maxWidth: .infinity)
            .frame(height: 44)
            .overlay(alignment: .trailing) {
                Button {
                    viewModel.cancel()
                    dismiss()
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 20, weight: .medium))
                        .frame(width: 44, height: 44)
                        .background(.bgSurfaceContainer, in: Circle())
                }
                .accessibilityLabel(String(localized: .reportIssueClose))
            }
    }

    var descriptionField: some View {
        VStack(alignment: .trailing, spacing: 8) {
            ZStack(alignment: .topLeading) {
                if viewModel.description.isEmpty {
                    Text(String(localized: .reportIssuePlaceholder))
                        .foregroundStyle(.fgSecondary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 8)
                }
                TextEditor(text: $viewModel.description)
                    .scrollContentBackground(.hidden)
                    .focused($isEditing)
                    .disabled(viewModel.isSending)
                    .accessibilityLabel(String(localized: .reportIssueQuestion))
                    .onChange(of: viewModel.description) { _, description in
                        viewModel.description = String(description.prefix(IssueReportViewModel.descriptionLimit))
                    }
            }
            .typography(.bodyLarge)
            .frame(height: 180)
            Text(verbatim: "\(viewModel.description.count) / \(IssueReportViewModel.descriptionLimit)")
                .typography(.labelMedium)
                .foregroundStyle(.fgSecondary)
        }
        .padding(16)
        .background(.bgSurfaceContainer, in: RoundedRectangle(cornerRadius: 24))
    }

    var sendButton: some View {
        Button {
            isEditing = false
            viewModel.send()
        } label: {
            HStack(spacing: 8) {
                if viewModel.isSending {
                    ProgressView().tint(.fgPrimaryInverted)
                }
                Text(String(localized: viewModel.isSending ? .reportIssueSending : .reportIssueSend))
                    .typography(.titleMedium.emphasized)
            }
            .frame(maxWidth: .infinity)
            .frame(height: 56)
            .foregroundStyle(.fgPrimaryInverted)
            .background(.fgPrimary, in: Capsule())
            .opacity(viewModel.canSend || viewModel.isSending ? 1 : 0.4)
        }
        .disabled(!viewModel.canSend)
    }
}
