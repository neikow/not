import NotCore
import SwiftUI

struct ContentView: View {
    @Bindable var model: AppModel

    /// Notes and workouts have no inspector; the editor takes the room.
    private var hasInspector: Bool {
        switch model.selection {
        case .notes, .workouts: false
        default: true
        }
    }

    private var showsInspector: Binding<Bool> {
        hasInspector ? $model.showsInspector : .constant(false)
    }

    var body: some View {
        NavigationSplitView {
            Sidebar(model: model)
                .navigationSplitViewColumnWidth(min: 190, ideal: 220)
        } detail: {
            Group {
                switch model.selection {
                case .habits: HabitsView(model: model)
                case .notes: NotesView(model: model)
                case .workouts: WorkoutsView(model: model)
                default: TaskListView(model: model)
                }
            }
            .inspector(isPresented: showsInspector) {
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
            ToolbarItem(placement: .primaryAction) { SyncAlert(state: model.syncState) }
            if hasInspector {
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        model.showsInspector.toggle()
                    } label: {
                        Label("Info", systemImage: "sidebar.trailing")
                    }
                    .help("Show details (⌘I)")
                }
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
    @State private var sharing: TaskList?

    var body: some View {
        List(selection: $model.selection) {
            Label("Today", systemImage: "calendar")
                .badge(model.dueCount)
                .tag(SidebarItem.today)
            Label("Upcoming", systemImage: "calendar.badge.clock")
                .badge(model.upcoming.count)
                .tag(SidebarItem.upcoming)
            if let inbox = model.inbox {
                Label(inbox.title, systemImage: "tray")
                    .badge(Int(inbox.openCount))
                    .tag(SidebarItem.list(inbox.id))
            }
            Label("Habits", systemImage: "repeat")
                .badge(model.habits.count { $0.pending })
                .tag(SidebarItem.habits)
            Label("Workouts", systemImage: "dumbbell")
                .tag(SidebarItem.workouts)
            Section("Notes") {
                Label("All Notes", systemImage: "note.text")
                    .badge(model.noteCount)
                    .tag(SidebarItem.notes(nil))
                    .dropDestination(for: String.self) { ids, _ in
                        ids.forEach { model.moveNote($0, to: nil) }
                        return true
                    }
                ForEach(model.folders, id: \.id) { folder in
                    FolderRow(folder: folder, renamingID: $renamingID, model: model)
                        .badge(Int(folder.noteCount))
                        .padding(.leading, CGFloat(folder.depth) * 14)
                        .tag(SidebarItem.notes(folder.id))
                        .dropDestination(for: String.self) { ids, _ in
                            for id in ids {
                                if model.folders.contains(where: { $0.id == id }) {
                                    model.moveFolder(id, into: folder.id)
                                } else {
                                    model.moveNote(id, to: folder.id)
                                }
                            }
                            return true
                        }
                        .draggable(folder.id)
                        .contextMenu {
                            Button("Rename") { renamingID = folder.id }
                            Button("New Folder Inside") { renamingID = model.addFolder(in: folder.id) }
                            if folder.parentId != nil {
                                Button("Move to Top Level") { model.moveFolder(folder.id, into: nil) }
                            }
                            Divider()
                            Button("Delete Folder", role: .destructive) { model.deleteFolder(folder.id) }
                        }
                }
            }
            Section("Lists") {
                ForEach(model.userLists, id: \.id) { list in
                    ListRow(list: list, renamingID: $renamingID, model: model)
                        .badge(Int(list.openCount))
                        .tag(SidebarItem.list(list.id))
                        .contextMenu {
                            Button("Rename") { renamingID = list.id }
                            Button("Share…") { sharing = list }
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
            HStack {
                Button {
                    renamingID = model.addList("New List")
                } label: {
                    Label("New List", systemImage: "plus")
                }
                .keyboardShortcut("n", modifiers: [.command, .shift])
                Spacer()
                Button {
                    renamingID = model.addFolder()
                } label: {
                    Label("New Folder", systemImage: "folder.badge.plus")
                }
                .labelStyle(.iconOnly)
                .help("New notes folder")
            }
            .buttonStyle(.borderless)
            .padding(12)
        }
        .sheet(item: $sharing) { list in ShareSheet(model: model, list: list) }
    }
}

/// Links that show a list in a browser, read only or editable, for people without the app.
private struct ShareSheet: View {
    let model: AppModel
    let list: TaskList
    @Environment(\.dismiss) private var dismiss
    @State private var shares: [ListShare]?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Share “\(list.title)”").font(.headline)
            if model.syncInfo == nil {
                Text("Links are served by your sync server: pair this Mac first (Settings → Sync).")
                    .foregroundStyle(.secondary)
            } else {
                Text("Anyone with a link sees this list in a browser, no app needed.")
                    .foregroundStyle(.secondary)
                if let shares {
                    ForEach(shares, id: \.id) { share in
                        HStack {
                            VStack(alignment: .leading) {
                                Text(share.canEdit ? "Can tick off and add" : "View only")
                                Text(share.url).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                            }
                            Spacer()
                            Button("Copy") {
                                NSPasteboard.general.clearContents()
                                NSPasteboard.general.setString(share.url, forType: .string)
                            }
                            Button(role: .destructive) {
                                Task {
                                    await model.revokeShare(share)
                                    self.shares = await model.shares(of: list.id)
                                }
                            } label: { Image(systemName: "trash") }
                                .buttonStyle(.borderless)
                                .help("Stop sharing")
                        }
                    }
                } else {
                    ProgressView().controlSize(.small)
                }
                HStack {
                    Button("New Link: Can Edit") { create(canEdit: true) }
                    Button("New Link: View Only") { create(canEdit: false) }
                }
            }
            HStack {
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 460)
        .task { if model.syncInfo != nil { shares = await model.shares(of: list.id) } }
    }

    private func create(canEdit: Bool) {
        Task {
            if let share = await model.shareList(list.id, canEdit: canEdit) {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(share.url, forType: .string)
                shares = (shares ?? []) + [share]
            }
        }
    }
}

private struct FolderRow: View {
    let folder: Folder
    @Binding var renamingID: String?
    let model: AppModel
    @State private var draft = ""
    @FocusState private var focused: Bool

    var body: some View {
        if renamingID == folder.id {
            TextField("Folder name", text: $draft)
                .focused($focused)
                .onAppear {
                    draft = folder.title
                    focused = true
                }
                .onSubmit(commit)
                .onExitCommand { renamingID = nil }
                .onChange(of: focused) { _, isFocused in if !isFocused { commit() } }
        } else {
            Label(folder.title, systemImage: "folder")
        }
    }

    private func commit() {
        guard renamingID == folder.id else { return }
        if draft != folder.title { model.renameFolder(folder.id, to: draft) }
        renamingID = nil
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
    @State private var draft = TaskDraft()
    @State private var focusRequests = 0
    @AppStorage("showsCompleted") private var showsCompleted = false

    private var parsed: ParsedTask? { model.parse(draft.text, dismissed: draft.dismissed) }

    private var isEmpty: Bool {
        model.outline.open.isEmpty && model.outline.completed.isEmpty && !(model.isToday && !model.todayHabits.isEmpty)
    }

    /// The last open row under the task getting a subtask: the inline field goes after it.
    private var subtaskAnchorID: String? {
        let rows = model.outline.open
        guard let parent = model.addingSubtaskTo,
              let start = rows.firstIndex(where: { $0.task.id == parent })
        else { return nil }
        let depth = rows[start].depth
        let end = rows[(start + 1)...].firstIndex { $0.depth <= depth } ?? rows.endIndex
        return rows[end - 1].task.id
    }

    var body: some View {
        VStack(spacing: 0) {
            addField
            Divider()
            if isEmpty {
                if model.isUpcoming {
                    ContentUnavailableView(
                        "Nothing coming up",
                        systemImage: "calendar.badge.clock",
                        description: Text("Tasks with a date after today show here, soonest first.")
                    )
                    .frame(maxHeight: .infinity)
                } else if model.isToday {
                    ContentUnavailableView(
                        "Nothing due today",
                        systemImage: "calendar",
                        description: Text("Add “tomorrow”, “fri 9am” or “every mon” to a task to see it here when it's due.")
                    )
                    .frame(maxHeight: .infinity)
                } else {
                    ContentUnavailableView(
                        "Nothing here yet",
                        systemImage: "checkmark.circle",
                        description: Text("Type above and press Return. Try “call mum fri 6pm #home !1”.")
                    )
                    .frame(maxHeight: .infinity)
                }
            } else {
                taskList
            }
        }
        // Keeps the add field at the top when the empty state alone wouldn't fill the column.
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .navigationTitle(model.title)
        .searchable(text: Binding(get: { model.taskSearch }, set: { model.taskSearch = $0 }), prompt: "Search tasks")
        .toolbar {
            // Today has its own order: overdue first, then by time.
            if !model.isDateView, let list = model.selectedList {
                ToolbarItem(placement: .primaryAction) {
                    Menu {
                        Picker("Sort by", selection: Binding(get: { list.sort }, set: { model.setSort(list.id, $0) })) {
                            ForEach([TaskSort.manual, .due, .priority, .title], id: \.self) { Text($0.label).tag($0) }
                        }
                        .pickerStyle(.inline)
                    } label: {
                        Label("Sort", systemImage: "arrow.up.arrow.down")
                    }
                    .help(list.sort == .manual ? "Sort tasks" : "Sorted by \(list.sort.label.lowercased()); dragging a task switches back to Manual")
                }
            }
        }
        .onAppear { focusRequests += 1 }
        .onChange(of: model.addFieldFocusRequests) { focusRequests += 1 }
    }

    private var addField: some View {
        let parsed = parsed
        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                Image(systemName: "plus.circle.fill")
                    .foregroundStyle(priorityColor(parsed?.priority ?? 0) ?? .accentColor)
                    .imageScale(.large)
                TaskInputField(
                    text: $draft.text,
                    placeholder: model.isToday ? "New task for today" : model.isUpcoming ? "New task for tomorrow" : "New task",
                    tokens: parsed?.tokens ?? [],
                    focusRequests: focusRequests + model.addFieldFocusRequests
                ) {
                    model.quickAdd(draft.text, dismissed: draft.dismissed)
                    draft.clear()
                }
            }
            TokenChips(parsed: parsed, model: model) { draft.dismissed.append($0.text) }
                .padding(.leading, 30)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .onChange(of: draft.text) { _, text in if text.isEmpty { draft.dismissed = [] } }
    }

    private var taskList: some View {
        List(selection: $model.selectedTaskID) {
            // Today's habits lead, as rows like the tasks under them.
            if model.isToday {
                ForEach(model.todayHabits, id: \.habit.id) { summary in
                    TodayHabitRow(summary: summary, model: model)
                        .selectionDisabled()
                        .moveDisabled(true)
                }
            }
            ForEach(model.outline.open, id: \.task.id) { row in
                TaskRowView(row: row, model: model, showsList: model.isDateView)
                if row.task.id == subtaskAnchorID, let parent = model.addingSubtaskTo,
                   let depth = model.outline.open.first(where: { $0.task.id == parent })?.depth {
                    SubtaskComposer(model: model, parentID: parent)
                        .padding(.leading, CGFloat(depth + 1) * 22)
                        .selectionDisabled()
                        .moveDisabled(true)
                }
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
                if !model.isDateView, model.outline.open.contains(where: { $0.task.id == id }) {
                    Button("Add Subtask") { model.beginSubtask(of: id) }
                    Button("Indent") {
                        model.selectedTaskID = id
                        model.indentSelected()
                    }
                    Button("Outdent") {
                        model.selectedTaskID = id
                        model.outdentSelected()
                    }
                }
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
        // Tab nests the selected task under the one above; ⇧Tab moves it back out.
        .onKeyPress(.tab, phases: .down) { press in
            guard model.editingTaskID == nil, model.addingSubtaskTo == nil, model.selectedTask != nil else { return .ignored }
            press.modifiers.contains(.shift) ? model.outdentSelected() : model.indentSelected()
            return .handled
        }
        .onKeyPress(keys: [.upArrow, .downArrow], phases: .down) { press in
            guard press.modifiers == .command, model.editingTaskID == nil else { return .ignored }
            model.moveSelected(by: press.key == .upArrow ? -1 : 1)
            return .handled
        }
    }
}

/// A habit on Today, shaped like a task row: its ring checks in (showing the goal's progress),
/// double-clicking opens it in Habits. Done ones stay, struck through.
private struct TodayHabitRow: View {
    let summary: HabitSummary
    let model: AppModel

    private var habit: Habit { summary.habit }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Button { withAnimation(.snappy) { model.toggleHabit(habit.id) } } label: {
                HabitRing(summary: summary, size: 15)
            }
            .buttonStyle(.plain)
            .alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + 4 }
            .help(summary.doneToday ? "Done today. Click to take it back." : "Check in")
            VStack(alignment: .leading, spacing: 1) {
                Text(habit.title)
                    .strikethrough(summary.doneToday)
                    .foregroundStyle(summary.doneToday ? .secondary : .primary)
                HStack(spacing: 4) {
                    Image(systemName: "repeat")
                    Text(scheduleText(habit, summary))
                    if summary.streak > 0, !summary.doneToday {
                        Label("\(summary.streak)\(streakUnit(habit.period))", systemImage: "flame.fill")
                            .foregroundStyle(.orange)
                    }
                }
                .font(.caption)
                .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .onTapGesture(count: 2) { model.openHabit(habit.id) }
        .contextMenu {
            Button("Check In") { model.logHabit(habit.id, 1) }.disabled(summary.doneToday)
            Button("Take One Back") { model.logHabit(habit.id, -1) }.disabled(summary.count == 0)
            Divider()
            Button("Show in Habits") { model.openHabit(habit.id) }
        }
    }
}

private struct TaskRowView: View {
    let row: OutlineRow
    let model: AppModel
    var showsList = false
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
                VStack(alignment: .leading, spacing: 1) {
                    Text(task.title)
                        .strikethrough(row.done)
                        .foregroundStyle(row.done ? .secondary : .primary)
                    meta
                }
            }
            if let color = priorityColor(task.priority), !row.done {
                Image(systemName: "flag.fill")
                    .imageScale(.small)
                    .foregroundStyle(color)
                    .help("\(priorityName(task.priority)) priority")
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

    /// "Tomorrow 9:00 ⟳ · Groceries": red once overdue, accent when due today.
    @ViewBuilder private var meta: some View {
        let label = scheduleLabel(due: task.due, time: task.dueTime, repeat: nil, today: model.today)
        if label != nil || showsList {
            HStack(spacing: 4) {
                if let label, let due = task.due {
                    Text(label)
                        .foregroundStyle(row.done ? AnyShapeStyle(.tertiary) : due < model.today ? AnyShapeStyle(.red) : due == model.today ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                    if task.repeat != nil { Image(systemName: "repeat").imageScale(.small).foregroundStyle(.secondary) }
                }
                if showsList {
                    Text(label == nil ? model.listTitle(task.listId) : "· \(model.listTitle(task.listId))")
                        .foregroundStyle(.tertiary)
                }
            }
            .font(.caption)
        }
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
    @State private var schedule = TaskDraft()
    @FocusState private var notesFocused: Bool
    @FocusState private var subtaskFocused: Bool

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

            Section {
                LabeledContent("Due") {
                    HStack {
                        Text(scheduleLabel(due: task.due, time: task.dueTime, repeat: task.repeat, today: model.today) ?? "None")
                            .foregroundStyle(task.due == nil ? .secondary : .primary)
                        if task.due != nil {
                            Button("Clear") { model.setSchedule(task.id, "") }
                                .buttonStyle(.link)
                        }
                    }
                }
                TaskInputField(
                    text: $schedule.text,
                    placeholder: "Change: fri 9am, every mon, 12 dec…",
                    tokens: model.parse(schedule.text, dismissed: schedule.dismissed)?.tokens ?? [],
                    font: .preferredFont(forTextStyle: .body)
                ) {
                    model.setSchedule(task.id, schedule.text)
                    schedule.clear()
                }
                Picker("Priority", selection: Binding(
                    get: { task.priority },
                    set: { model.setPriority(task.id, $0) }
                )) {
                    ForEach([UInt8(3), 2, 1, 0], id: \.self) { p in
                        Label(priorityName(p), systemImage: p == 0 ? "flag.slash" : "flag.fill")
                            .foregroundStyle(priorityColor(p) ?? .secondary)
                            .tag(p)
                    }
                }
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
                    SubtaskRow(model: model, subtask: subtask)
                }
                HStack(spacing: 8) {
                    Image(systemName: "plus")
                        .foregroundStyle(.secondary)
                        .frame(width: 14)
                    TextField("Add subtask", text: $subtaskDraft, prompt: Text("Add subtask"))
                        .labelsHidden()
                        .textFieldStyle(.plain)
                        .focused($subtaskFocused)
                        .onSubmit {
                            model.addTask(subtaskDraft, to: task.id)
                            subtaskDraft = ""
                            // Stays in the field for the next one.
                            subtaskFocused = true
                        }
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

/// A subtask in the inspector: tick it, edit its title in place, or remove it.
private struct SubtaskRow: View {
    let model: AppModel
    let subtask: TaskItem
    @State private var title = ""
    @State private var hovering = false
    @FocusState private var focused: Bool

    private var done: Bool { subtask.completedAt != nil }

    var body: some View {
        HStack(spacing: 8) {
            Toggle("", isOn: Binding(get: { done }, set: { _ in model.toggle(subtask) }))
                .toggleStyle(.checkbox)
                .labelsHidden()
            TextField("Subtask", text: $title)
                .labelsHidden()
                .textFieldStyle(.plain)
                .strikethrough(done)
                .foregroundStyle(done ? .secondary : .primary)
                .focused($focused)
                .onSubmit(commit)
                .onChange(of: focused) { _, isFocused in if !isFocused { commit() } }
                .onKeyPress(.delete) {
                    // ⌫ in an empty title removes the subtask.
                    guard title.isEmpty else { return .ignored }
                    model.delete(id: subtask.id)
                    return .handled
                }
            Button { model.delete(id: subtask.id) } label: {
                Image(systemName: "xmark.circle.fill")
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.tertiary)
            .opacity(hovering ? 1 : 0)
            .help("Delete subtask")
        }
        .onHover { hovering = $0 }
        .onAppear { title = subtask.title }
        .onChange(of: subtask.title) { _, synced in if !focused { title = synced } }
    }

    private func commit() {
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { title = subtask.title } else if trimmed != subtask.title { model.rename(subtask.id, to: trimmed) }
    }
}

/// The inline field under a task in the list: Return adds a subtask and stays for the next one;
/// Esc, or Return on an empty field, closes it.
private struct SubtaskComposer: View {
    let model: AppModel
    let parentID: String
    @State private var draft = ""
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "circle.dashed")
                .foregroundStyle(.tertiary)
            TextField("New subtask", text: $draft)
                .textFieldStyle(.plain)
                .focused($focused)
                .onAppear { focused = true }
                .onSubmit {
                    guard !draft.trimmingCharacters(in: .whitespaces).isEmpty else { return close() }
                    model.quickAdd(draft, to: parentID)
                    draft = ""
                    focused = true
                }
                .onExitCommand(perform: close)
                .onChange(of: focused) { _, isFocused in
                    if !isFocused, draft.isEmpty { close() }
                }
        }
        .padding(.vertical, 2)
    }

    private func close() {
        if model.addingSubtaskTo == parentID { model.addingSubtaskTo = nil }
    }
}

// MARK: - Shared

/// The toolbar's sync status: quiet unless something needs attention (offline, signed out, or a
/// sync that takes a while). Opens Settings. The menu bar panel shows the full `SyncBadge`.
struct SyncAlert: View {
    let state: SyncState?
    @Environment(\.openSettings) private var openSettings
    @State private var slow = false

    var body: some View {
        Group {
            switch state {
            case .offline, .unauthorized: button
            case .syncing where slow: button
            default: EmptyView()
            }
        }
        .task(id: state) {
            slow = false
            guard state == .syncing else { return }
            try? await Task.sleep(for: .seconds(1.5))
            slow = !Task.isCancelled
        }
    }

    private var button: some View {
        Button { openSettings() } label: { SyncBadge(state: state) }
    }
}

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

extension TaskSort {
    var label: String {
        switch self {
        case .manual: "Manual"
        case .due: "Due date"
        case .priority: "Priority"
        case .title: "Title"
        }
    }
}

extension TaskList: @retroactive Identifiable {}
