import AppKit
import NotCore
import SwiftUI

/// The menu bar extra: add a task or tick one off without switching apps.
struct MenuBarPanel: View {
    static let visibleKey = "showsMenuBarExtra"

    @Bindable var model: AppModel
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings
    @State private var draft = ""
    @State private var listHeight: CGFloat = 0
    @FocusState private var fieldFocused: Bool

    private var listTitle: String { model.panelList?.title ?? "Inbox" }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            addField
            Divider()
            tasks
            Divider()
            footer
        }
        .frame(width: 320)
        .onAppear { fieldFocused = true }
    }

    private var header: some View {
        HStack {
            Menu {
                ForEach(model.lists, id: \.id) { list in
                    Button(list.title) { model.panelListID = list.id }
                }
            } label: {
                Text(listTitle).font(.headline)
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            Spacer()
            SyncBadge(state: model.syncState)
        }
        .padding(.horizontal, 14)
        .padding(.top, 12)
        .padding(.bottom, 8)
    }

    private var addField: some View {
        TextField("Add to \(listTitle)", text: $draft)
            .textFieldStyle(.plain)
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(.quaternary.opacity(0.6), in: RoundedRectangle(cornerRadius: 8))
            .focused($fieldFocused)
            .onSubmit {
                model.quickAdd(draft, to: model.panelListID)
                draft = ""
            }
            .padding(.horizontal, 12)
            .padding(.bottom, 12)
    }

    @ViewBuilder
    private var tasks: some View {
        if model.panelRows.isEmpty {
            Label("Nothing to do", systemImage: "checkmark.circle")
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: 72)
        } else {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(model.panelRows, id: \.task.id) { row in
                        PanelTaskRow(row: row) { model.toggle(row.task) }
                            .transition(.opacity)
                    }
                }
                .padding(.vertical, 6)
                .animation(.snappy, value: model.panelRows.map(\.task.id))
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { listHeight = $0 }
            }
            // As tall as the tasks, up to a limit; scrolls beyond it.
            .frame(height: min(max(listHeight, 1), 360))
            .scrollBounceBehavior(.basedOnSize)
        }
    }

    private var footer: some View {
        HStack(spacing: 14) {
            Button("Open not") {
                openWindow(id: MainWindow.id)
                NSApp.activate()
            }
            .keyboardShortcut("o")
            Spacer()
            Button {
                openSettings()
                NSApp.activate()
            } label: {
                Image(systemName: "gearshape")
            }
            .help("Settings")
            .keyboardShortcut(",")
            Button {
                NSApp.terminate(nil)
            } label: {
                Image(systemName: "power")
            }
            .help("Quit not")
            .keyboardShortcut("q")
        }
        .buttonStyle(.borderless)
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }
}

/// A row that shows the tick for a moment before the task leaves the list.
private struct PanelTaskRow: View {
    let row: OutlineRow
    let complete: () -> Void
    @State private var ticked = false
    @State private var hovering = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Toggle("", isOn: Binding(get: { ticked }, set: { if $0 { tick() } }))
                .toggleStyle(.checkbox)
                .labelsHidden()
                .disabled(ticked)
            Text(row.task.title)
                .lineLimit(2)
                .strikethrough(ticked)
                .foregroundStyle(ticked ? .secondary : .primary)
            Spacer(minLength: 0)
        }
        .padding(.leading, 8 + CGFloat(row.depth) * 18)
        .padding(.trailing, 8)
        .padding(.vertical, 5)
        .background(hovering ? Color.primary.opacity(0.07) : .clear, in: RoundedRectangle(cornerRadius: 6))
        .padding(.horizontal, 6)
        .onHover { hovering = $0 }
    }

    private func tick() {
        withAnimation(.snappy) { ticked = true }
        Task {
            try? await Task.sleep(for: .milliseconds(600))
            complete()
        }
    }
}
