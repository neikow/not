import NotCore
import SwiftUI

struct ContentView: View {
    @Bindable var model: AppModel

    var body: some View {
        NavigationSplitView {
            Sidebar(model: model)
                .navigationSplitViewColumnWidth(min: 190, ideal: 220)
        } detail: {
            Group {
                if model.selection == .habits {
                    HabitsView(model: model)
                } else {
                    TaskListView(model: model)
                }
            }
            .inspector(isPresented: $model.showsInspector) {
                Group {
                    if model.selection == .habits {
                        HabitInspector(model: model)
                    } else {
                        TaskInspector(model: model)
                    }
                }
                .inspectorColumnWidth(min: 260, ideal: 300, max: 420)
            }
        }
        .toolbar {
            ToolbarItem(placement: .status) { SyncBadge(state: model.syncState) }
            ToolbarItem(placement: .primaryAction) {
                Button {
                    model.showsInspector.toggle()
                } label: {
                    Label("Info", systemImage: "sidebar.trailing")
                }
                .help("Show details (⌘I)")
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .NSCalendarDayChanged)) { _ in
            model.refreshToday()
        }
        .alert("Something went wrong", isPresented: .constant(model.errorMessage != nil)) {
            Button("OK") { model.errorMessage = nil }
        } message: {
            Text(model.errorMessage ?? "")
        }
    }
}

// MARK: - Sidebar

private struct Sidebar: View {
    @Bindable var model: AppModel
    @State private var renamingID: String?

    var body: some View {
        List(selection: $model.selection) {
            if let inbox = model.inbox {
                Label(inbox.title, systemImage: "tray")
                    .badge(Int(inbox.openCount))
                    .tag(SidebarItem.list(inbox.id))
            }
            Label("Habits", systemImage: "repeat")
                .badge(model.habits.count { $0.due && $0.count < $0.habit.goal })
                .tag(SidebarItem.habits)
            Section("Lists") {
                ForEach(model.userLists, id: \.id) { list in
                    ListRow(list: list, renamingID: $renamingID, model: model)
                        .badge(Int(list.openCount))
                        .tag(SidebarItem.list(list.id))
                        .contextMenu {
                            Button("Rename") { renamingID = list.id }
                            Divider()
                            Button("Delete List", role: .destructive) { model.delete(id: list.id) }
                        }
                }
                .onMove { source, destination in
                    guard let from = source.first else { return }
                    // `destination` counts the moved row itself when moving down.
                    model.moveList(model.userLists[from].id, to: from < destination ? destination - 1 : destination)
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            Button {
                renamingID = model.addList("New List")
            } label: {
                Label("New List", systemImage: "plus")
            }
            .buttonStyle(.borderless)
            .keyboardShortcut("n", modifiers: [.command, .shift])
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(12)
        }
    }
}

private struct ListRow: View {
    let list: TaskList
    @Binding var renamingID: String?
    let model: AppModel
    @State private var draft = ""
    @FocusState private var focused: Bool

    var body: some View {
        if renamingID == list.id {
            TextField("List name", text: $draft)
                .focused($focused)
                .onAppear {
                    draft = list.title
                    focused = true
                }
                .onSubmit(commit)
                .onExitCommand { renamingID = nil }
                .onChange(of: focused) { _, isFocused in if !isFocused { commit() } }
        } else {
            Label(list.title, systemImage: "list.bullet")
        }
    }

    private func commit() {
        guard renamingID == list.id else { return }
        if draft != list.title { model.rename(list.id, to: draft) }
        renamingID = nil
    }
}

// MARK: - Tasks

private struct TaskListView: View {
    @Bindable var model: AppModel
    @State private var draft = ""
    @AppStorage("showsCompleted") private var showsCompleted = false
    @FocusState private var addFieldFocused: Bool

    private var isEmpty: Bool { model.outline.open.isEmpty && model.outline.completed.isEmpty }

    var body: some View {
        VStack(spacing: 0) {
            addField
            Divider()
            if isEmpty {
                ContentUnavailableView(
                    "Nothing here yet",
                    systemImage: "checkmark.circle",
                    description: Text("Type above and press Return.")
                )
            } else {
                taskList
            }
        }
        .navigationTitle(model.selectedList?.title ?? "not")
        .onAppear { addFieldFocused = true }
        .onChange(of: model.addFieldFocusRequests) { addFieldFocused = true }
    }

    private var addField: some View {
        HStack(spacing: 10) {
            Image(systemName: "plus.circle.fill")
                .foregroundStyle(.tint)
                .imageScale(.large)
            TextField("New task", text: $draft)
                .textFieldStyle(.plain)
                .font(.title3)
                .focused($addFieldFocused)
                .onSubmit {
                    model.addTask(draft)
                    draft = ""
                }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
    }

    private var taskList: some View {
        List(selection: $model.selectedTaskID) {
            ForEach(model.outline.open, id: \.task.id) { row in
                TaskRowView(row: row, model: model)
            }
            .onMove { source, destination in
                if let from = source.first { model.moveOpenRow(from: from, to: destination) }
            }

            if !model.outline.completed.isEmpty {
                // `Section(isExpanded:)` only gets a disclosure control in sidebar lists, so the
                // header toggles it itself.
                Section {
                    if showsCompleted {
                        ForEach(model.outline.completed, id: \.task.id) { row in
                            TaskRowView(row: row, model: model)
                        }
                    }
                } header: {
                    Button {
                        withAnimation(.snappy) { showsCompleted.toggle() }
                    } label: {
                        HStack(spacing: 4) {
                            Image(systemName: "chevron.right")
                                .rotationEffect(.degrees(showsCompleted ? 90 : 0))
                                .imageScale(.small)
                            Text("Completed · \(model.outline.completed.count { $0.depth == 0 })")
                        }
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .listStyle(.inset)
        .animation(.snappy, value: model.outline.open.map(\.task.id))
        .animation(.snappy, value: model.outline.completed.map(\.task.id))
        .contextMenu(forSelectionType: String.self) { ids in
            if let id = ids.first {
                Button("Edit Title") { model.editingTaskID = id }
                Button("Show Info") {
                    model.selectedTaskID = id
                    model.showsInspector = true
                }
                Divider()
                Button("Delete", role: .destructive) { model.delete(id: id) }
            }
        } primaryAction: { ids in
            model.editingTaskID = ids.first
        }
        .onDeleteCommand { model.deleteSelected() }
        .onKeyPress(.space) {
            guard model.editingTaskID == nil, model.selectedTask != nil else { return .ignored }
            withAnimation(.snappy) { model.toggleSelected() }
            return .handled
        }
        .onKeyPress(.return) {
            guard model.editingTaskID == nil, let id = model.selectedTaskID else { return .ignored }
            model.editingTaskID = id
            return .handled
        }
        .onKeyPress(keys: [.upArrow, .downArrow], phases: .down) { press in
            guard press.modifiers == .command, model.editingTaskID == nil else { return .ignored }
            model.moveSelected(by: press.key == .upArrow ? -1 : 1)
            return .handled
        }
    }
}

private struct TaskRowView: View {
    let row: OutlineRow
    let model: AppModel
    @State private var draft = ""
    @FocusState private var focused: Bool

    private var task: TaskItem { row.task }
    private var isEditing: Bool { model.editingTaskID == task.id }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Toggle("", isOn: Binding(
                get: { task.completedAt != nil },
                set: { _ in withAnimation(.snappy) { model.toggle(task) } }
            ))
            .toggleStyle(.checkbox)
            .labelsHidden()

            if isEditing {
                TextField("Title", text: $draft)
                    .textFieldStyle(.plain)
                    .focused($focused)
                    .onAppear {
                        draft = task.title
                        focused = true
                    }
                    .onSubmit(commit)
                    .onExitCommand { model.editingTaskID = nil }
                    .onChange(of: focused) { _, isFocused in if !isFocused { commit() } }
            } else {
                Text(task.title)
                    .strikethrough(row.done)
                    .foregroundStyle(row.done ? .secondary : .primary)
            }
            if !task.notes.isEmpty {
                Image(systemName: "text.alignleft")
                    .imageScale(.small)
                    .foregroundStyle(.tertiary)
                    .help(task.notes)
            }
            Spacer(minLength: 0)
        }
        .padding(.leading, CGFloat(row.depth) * 22)
        .padding(.vertical, 2)
    }

    private func commit() {
        guard isEditing else { return }
        if draft != task.title { model.rename(task.id, to: draft) }
        model.editingTaskID = nil
    }
}

// MARK: - Inspector

private struct TaskInspector: View {
    let model: AppModel

    var body: some View {
        if let task = model.selectedTask {
            TaskDetail(model: model, task: task)
                .id(task.id)
        } else {
            ContentUnavailableView("No Task Selected", systemImage: "sidebar.trailing")
        }
    }
}

private struct TaskDetail: View {
    let model: AppModel
    let task: TaskItem
    @State private var title = ""
    @State private var notes = ""
    @State private var subtaskDraft = ""
    @FocusState private var notesFocused: Bool

    var body: some View {
        Form {
            Section {
                TextField("Title", text: $title, axis: .vertical)
                    .font(.title3.weight(.semibold))
                    .onSubmit { model.rename(task.id, to: title) }
                Picker("List", selection: Binding(
                    get: { task.listId },
                    set: { model.move(task.id, toList: $0) }
                )) {
                    ForEach(model.lists, id: \.id) { Text($0.title).tag($0.id) }
                }
                .disabled(task.parentId != nil)
            }

            Section("Notes") {
                TextEditor(text: $notes)
                    .font(.body)
                    .frame(minHeight: 120)
                    .scrollContentBackground(.hidden)
                    .focused($notesFocused)
            }

            Section("Subtasks") {
                ForEach(model.subtasks(of: task.id), id: \.id) { subtask in
                    Toggle(isOn: Binding(
                        get: { subtask.completedAt != nil },
                        set: { _ in model.toggle(subtask) }
                    )) {
                        Text(subtask.title)
                            .strikethrough(subtask.completedAt != nil)
                            .foregroundStyle(subtask.completedAt != nil ? .secondary : .primary)
                    }
                    .toggleStyle(.checkbox)
                }
                TextField("Add subtask", text: $subtaskDraft)
                    .onSubmit {
                        model.addTask(subtaskDraft, to: task.id)
                        subtaskDraft = ""
                    }
            }

            Section {
                Button("Delete Task", role: .destructive) { model.delete(id: task.id) }
            }
        }
        .formStyle(.grouped)
        .onAppear {
            title = task.title
            notes = task.notes
        }
        // Saving pauses half a second after typing stops; each save is one undo step.
        .task(id: notes) {
            guard notes != task.notes else { return }
            try? await Task.sleep(for: .milliseconds(500))
            model.setNotes(task.id, notes)
        }
        .onChange(of: task.notes) { _, synced in if !notesFocused { notes = synced } }
        .onChange(of: task.title) { _, synced in title = synced }
        .onDisappear {
            // Switching tasks saves pending edits; a deleted task has nothing left to save.
            guard model.contains(task.id) else { return }
            if title != task.title { model.rename(task.id, to: title) }
            if notes != task.notes { model.setNotes(task.id, notes) }
        }
    }
}

// MARK: - Shared

struct SyncBadge: View {
    let state: SyncState?

    var body: some View {
        switch state {
        case nil:
            Image(systemName: "icloud.slash").help("Not paired — pair with a server in Settings (⌘,)")
        case .syncing:
            Image(systemName: "arrow.triangle.2.circlepath.icloud").help("Syncing…")
        case .synced:
            Image(systemName: "checkmark.icloud").help("Up to date")
        case let .offline(reason):
            Image(systemName: "exclamationmark.icloud").foregroundStyle(.orange).help("Offline: \(reason)")
        case .unauthorized:
            Image(systemName: "xmark.icloud").foregroundStyle(.red)
                .help("The server no longer accepts this Mac — pair again in Settings (⌘,)")
        }
    }
}
