import NotCore
import SwiftUI

struct ContentView: View {
    @Bindable var model: AppModel
    @State private var newList = ""

    var body: some View {
        NavigationSplitView {
            List(selection: $model.selectedListID) {
                Section("Lists") {
                    ForEach(model.lists, id: \.id) { list in
                        Label(list.title, systemImage: "list.bullet")
                            .tag(list.id)
                            .contextMenu {
                                Button("Delete List", role: .destructive) { model.delete(id: list.id) }
                            }
                    }
                }
            }
            .safeAreaInset(edge: .bottom) {
                TextField("New list", text: $newList)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit {
                        model.addList(newList)
                        newList = ""
                    }
                    .padding(10)
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 220)
        } detail: {
            TasksView(model: model)
        }
        .toolbar {
            ToolbarItem(placement: .status) { SyncBadge(state: model.syncState) }
        }
        .alert("Something went wrong", isPresented: .constant(model.errorMessage != nil)) {
            Button("OK") { model.errorMessage = nil }
        } message: {
            Text(model.errorMessage ?? "")
        }
    }
}

private struct TasksView: View {
    let model: AppModel
    @State private var draft = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            TextField("Add a task", text: $draft)
                .textFieldStyle(.plain)
                .font(.title3)
                .focused($focused)
                .onSubmit {
                    model.addTask(draft)
                    draft = ""
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            Divider()
            if model.rows.isEmpty {
                ContentUnavailableView("No tasks", systemImage: "checkmark.circle", description: Text("Type above and press Return."))
            } else {
                List(model.rows) { row in
                    TaskRowView(row: row, toggle: { model.toggle(row.task) })
                        .contextMenu {
                            Button("Delete", role: .destructive) { model.delete(id: row.task.id) }
                        }
                }
                .listStyle(.inset)
            }
        }
        .navigationTitle(model.selectedList?.title ?? "not")
        .onAppear { focused = true }
    }
}

private struct TaskRowView: View {
    let row: TaskRow
    let toggle: () -> Void

    private var done: Bool { row.task.completedAt != nil }

    var body: some View {
        HStack(spacing: 8) {
            Toggle("", isOn: Binding(get: { done }, set: { _ in toggle() }))
                .toggleStyle(.checkbox)
                .labelsHidden()
            Text(row.task.title)
                .strikethrough(done)
                .foregroundStyle(done ? .secondary : .primary)
        }
        .padding(.leading, CGFloat(row.depth) * 20)
    }
}

private struct SyncBadge: View {
    let state: SyncState?

    var body: some View {
        switch state {
        case nil:
            Image(systemName: "icloud.slash").help("Sync is off — set a server in Settings (⌘,)")
        case .syncing:
            Image(systemName: "arrow.triangle.2.circlepath.icloud").help("Syncing…")
        case .synced:
            Image(systemName: "checkmark.icloud").help("Up to date")
        case let .offline(reason):
            Image(systemName: "exclamationmark.icloud").foregroundStyle(.orange).help("Offline: \(reason)")
        }
    }
}
