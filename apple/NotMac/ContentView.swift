import NotCore
import SwiftUI

struct ContentView: View {
    @Bindable var model: AppModel
    /// The first-run steps, on a fresh Mac.
    @State private var onboarding = false
    /// What changed since the version last opened, shown once after an update.
    @State private var news: [Release] = []

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
        .onAppear {
            onboarding = FirstRun.needed(model)
            news = WhatsNew.pending(freshInstall: onboarding)
        }
        .sheet(isPresented: $onboarding) { OnboardingSheet(model: model) }
        .sheet(isPresented: Binding(get: { !onboarding && !news.isEmpty }, set: { if !$0 { news = [] } })) {
            WhatsNewSheet(releases: news)
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
    @State private var sharing: TaskView?
    @State private var editing: ViewDraft?
    @State private var renamingTag: String?
    @State private var tagDraft = ""
    @State private var reviewing = false
    @State private var showsLogbook = false

    var body: some View {
        List(selection: $model.selection) {
            ForEach(model.builtInViews, id: \.id) { view in
                Label(view.title, systemImage: builtInSymbol(view.id, model))
                    .badge(Int(view.openCount))
                    .tag(view.id == model.todayID ? SidebarItem.today : view.id == model.upcomingID ? .upcoming : .list(view.id))
            }
            Label("Habits", systemImage: "repeat")
                .badge(model.pendingHabits)
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
            Section("Smart Lists") {
                ForEach(model.userLists, id: \.id) { list in
                    Label(list.title, systemImage: "line.3.horizontal.decrease.circle")
                        .help(list.query)
                        .badge(Int(list.openCount))
                        .tag(SidebarItem.list(list.id))
                        .contextMenu {
                            Button("Edit…") { editing = ViewDraft(id: list.id, title: list.title, query: list.query) }
                            Button("Share…") { sharing = list }
                            Divider()
                            Button("Delete List", role: .destructive) { model.deleteList(list.id) }
                        }
                }
                .onMove { source, destination in
                    guard let from = source.first else { return }
                    // `destination` counts the moved row itself when moving down.
                    model.moveList(model.userLists[from].id, to: from < destination ? destination - 1 : destination)
                }
            }
            if !model.tags.isEmpty {
                Section("Tags") {
                    ForEach(model.tags, id: \.name) { tag in
                        Group {
                            if renamingTag == tag.name {
                                TextField("Tag", text: $tagDraft)
                                    .onSubmit {
                                        model.renameTag(tag.name, to: tagDraft)
                                        renamingTag = nil
                                    }
                                    .onExitCommand { renamingTag = nil }
                            } else {
                                Label(tag.name, systemImage: "number")
                            }
                        }
                        .badge(Int(tag.openCount))
                        .tag(SidebarItem.list("#" + tag.name))
                        .contextMenu {
                            Button("Make Smart List") {
                                editing = ViewDraft(title: tag.name.capitalized, query: "#" + tag.name)
                            }
                            Button("Rename Tag") {
                                tagDraft = tag.name
                                renamingTag = tag.name
                            }
                            Divider()
                            Button("Remove from All Tasks", role: .destructive) { model.deleteTag(tag.name) }
                        }
                    }
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            HStack {
                Button {
                    editing = ViewDraft()
                } label: {
                    Label("New List", systemImage: "plus")
                }
                .keyboardShortcut("n", modifiers: [.command, .shift])
                Spacer()
                Button { reviewing = true } label: { Label("Weekly Review", systemImage: "checklist") }
                    .labelStyle(.iconOnly)
                    .help("Weekly review")
                Button { showsLogbook = true } label: { Label("Logbook", systemImage: "book.closed") }
                    .labelStyle(.iconOnly)
                    .help("Logbook: everything completed, by day")
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
        .sheet(item: $editing) { draft in ViewEditor(model: model, draft: draft) }
        .sheet(isPresented: $reviewing) { ReviewSheet(model: model) }
        .sheet(isPresented: $showsLogbook) { LogbookSheet(model: model) }
        .sheet(isPresented: $model.processingInbox) { ProcessInboxSheet(model: model) }
    }
}

func builtInSymbol(_ id: String, _ model: AppModel) -> String {
    switch id {
    case model.todayID: "star"
    case model.upcomingID: "calendar"
    case model.somedayID: "cloud"
    default: "tray"
    }
}

/// Links that show a list in a browser, read only or editable, for people without the app.
private struct ShareSheet: View {
    let model: AppModel
    let list: TaskView
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

// MARK: - Tasks

private struct TaskListView: View {
    @Bindable var model: AppModel
    @State private var draft = TaskDraft()
    @State private var focusRequests = 0
    @AppStorage("showsCompleted") private var showsCompleted = false

    private var parsed: ParsedTask? { model.parse(draft.text, dismissed: draft.dismissed) }

    private var isEmpty: Bool {
        model.outline.open.isEmpty && model.outline.completed.isEmpty
            && !(model.isToday && (!model.todayHabits.isEmpty || !model.calendar.on(model.today).isEmpty))
            && !(model.isUpcoming && model.calendar.events.contains { $0.day > model.today })
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
            if !model.earlier.isEmpty {
                HStack {
                    Image(systemName: "clock.arrow.circlepath").foregroundStyle(.secondary)
                    Text(model.earlier.count == 1 ? "1 from an earlier day" : "\(model.earlier.count) from earlier days")
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Move to Today") { model.moveToToday(model.earlier.map(\.id)) }
                }
                .padding(.horizontal, 18)
                .padding(.vertical, 8)
                Divider()
            }
            if isEmpty {
                if model.isUpcoming {
                    ContentUnavailableView(
                        "Nothing coming up",
                        systemImage: "calendar",
                        description: Text("Tasks starting or due after today show here, soonest first.")
                    )
                    .frame(maxHeight: .infinity)
                } else if model.isToday {
                    ContentUnavailableView(
                        "Nothing for today",
                        systemImage: "star",
                        description: Text("Add “today”, “fri 9am”, “by mon” or “every mon” to a task to see it here on the day.")
                    )
                    .frame(maxHeight: .infinity)
                } else if model.selectedListID == model.somedayID {
                    ContentUnavailableView(
                        "Nothing for someday",
                        systemImage: "cloud",
                        description: Text("Park ideas and maybes here with “someday”, out of the way until you want them.")
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
            if model.selectedListID == model.inboxID, !model.outline.open.isEmpty {
                ToolbarItem(placement: .primaryAction) {
                    Button { model.processingInbox = true } label: { Label("Process", systemImage: "play.fill") }
                        .help("Process the Inbox one task at a time (⇧⌘P)")
                }
            }
            // Upcoming is always by date.
            if !model.isDateView, let id = model.selectedListID {
                let sort = model.selectedList?.sort ?? .manual
                ToolbarItem(placement: .primaryAction) {
                    Menu {
                        Picker("Sort by", selection: Binding(get: { sort }, set: { model.setSort(id, $0) })) {
                            ForEach([TaskSort.manual, .due, .priority, .title], id: \.self) { Text($0.label).tag($0) }
                        }
                        .pickerStyle(.inline)
                    } label: {
                        Label("Sort", systemImage: "arrow.up.arrow.down")
                    }
                    .help(sort == .manual ? "Sort tasks" : "Sorted by \(sort.label.lowercased()); dragging a task switches back to Manual")
                }
            }
        }
        .onAppear { focusRequests += 1 }
        .onChange(of: model.addFieldFocusRequests) { focusRequests += 1 }
        // A two-finger swipe sideways steps through the lists, like on the phone.
        .background(SwipeCatcher { forward in withAnimation(.snappy) { model.stepList(forward: forward) } })
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
                    placeholder: model.isToday ? "New task for today" : model.isUpcoming ? "New task — say when, like “fri”" : "New task in \(model.title)",
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
            // The day's calendar, read only, above what there is to do.
            if model.isToday {
                ForEach(model.calendar.on(model.today)) { event in
                    EventRow(event: event, overlay: model.calendar)
                        .selectionDisabled()
                        .moveDisabled(true)
                }
            }
            // Today's habits lead, as rows like the tasks under them.
            if model.isToday {
                ForEach(model.todayHabits, id: \.habit.id) { summary in
                    TodayHabitRow(summary: summary, model: model)
                        .selectionDisabled()
                        .moveDisabled(true)
                }
            }
            if model.isUpcoming {
                // By day, with the calendar's events of that day: Tomorrow, the weekdays, Later.
                ForEach(upcomingDays(model.outline.open, events: model.calendar.events, today: model.today), id: \.label) { day in
                    Section(day.label) {
                        ForEach(day.events) { event in
                            EventRow(event: event, overlay: model.calendar).selectionDisabled()
                        }
                        ForEach(day.rows, id: \.task.id) { row in
                            TaskRowView(row: row, model: model)
                        }
                    }
                }
            } else {
            ForEach(model.outline.open, id: \.task.id) { row in
                TaskRowView(row: row, model: model, hiddenTag: model.selectedListID.flatMap(soleTag))
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
                Menu("Snooze") {
                    ForEach(snoozes, id: \.0) { label, to in
                        Button(label) { model.snooze([id], to) }
                    }
                }
                Button("Move to Today") { model.moveToToday([id]) }
                if let task = (model.outline.open + model.outline.completed).first(where: { $0.task.id == id })?.task, !task.someday {
                    Button("Move to Someday") { model.setSomeday(task) }
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
        if habit.timed != nil {
            TodayTimedRow(summary: summary, model: model)
        } else {
            checkIns
        }
    }

    private var checkIns: some View {
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

/// A timed habit in Today: its ring, where it stands (counting live) and its switch, like the
/// Habits row but sized to sit among tasks.
private struct TodayTimedRow: View {
    let summary: HabitSummary
    let model: AppModel

    var body: some View {
        let habit = summary.habit
        TimelineView(.periodic(from: .now, by: 1)) { context in
            if let rule = habit.timed, let live = summary.timedLive(now: context.date, loadedAt: model.habitsLoadedAt) {
                HStack(alignment: .center, spacing: 8) {
                    TimedRingView(live: live, size: 15)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(habit.title)
                        Text(timedStatus(rule, live, now: context.date))
                            .font(.caption)
                            .foregroundStyle(live.atMostOff && live.leftMs < 0 ? AnyShapeStyle(.red)
                                : live.tracked && live.on != rule.usualOn ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
                    }
                    Spacer(minLength: 8)
                    TimedSwitchButton(rule: rule, live: live, size: .small) { model.switchHabit(habit.id, on: !live.on) }
                }
                .padding(.vertical, 2)
                .contentShape(Rectangle())
                .onTapGesture(count: 2) { model.openHabit(habit.id) }
                .contextMenu {
                    Button(switchLabel(rule, on: live.on)) { model.switchHabit(habit.id, on: !live.on) }
                    Divider()
                    Button("Show in Habits") { model.openHabit(habit.id) }
                }
            }
        }
    }
}

private struct TaskRowView: View {
    let row: OutlineRow
    let model: AppModel
    /// The tag the view is about, left out of the row.
    var hiddenTag: String?
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

    /// "Tomorrow 9:00 ⟳ · ⏳ Fri · 2/5 · #home": a deadline turns accent on the day and red after.
    @ViewBuilder private var meta: some View {
        TaskMeta(task: task, today: model.today, done: row.done, hiddenTag: hiddenTag, subtasks: (row.subtasks, row.subtasksDone))
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
    @State private var tags = ""
    @State private var deadline = ""
    @FocusState private var notesFocused: Bool
    @FocusState private var subtaskFocused: Bool

    var body: some View {
        Form {
            Section {
                TextField("Title", text: $title, axis: .vertical)
                    .font(.title3.weight(.semibold))
                    .onSubmit { model.rename(task.id, to: title) }
                TextField("Tags", text: $tags, prompt: Text("Tags: work, home…"))
                    .onSubmit { model.setTags(task.id, tags.split(whereSeparator: { $0 == " " || $0 == "," }).map(String.init)) }
            }

            Section {
                LabeledContent("When") {
                    HStack {
                        Text(task.someday ? "Someday" : scheduleLabel(due: task.start, time: task.time, repeat: task.repeat, today: model.today) ?? "None")
                            .foregroundStyle(task.start == nil && !task.someday ? .secondary : .primary)
                        if task.start != nil || task.someday {
                            Button("Clear") { model.setSchedule(task.id, "") }
                                .buttonStyle(.link)
                        }
                    }
                }
                LabeledContent("Deadline") {
                    HStack {
                        Text(task.deadline.flatMap { scheduleLabel(due: $0, time: nil, repeat: task.start == nil ? task.repeat : nil, today: model.today) } ?? "None")
                            .foregroundStyle(task.deadline.map { $0 < model.today } == true ? AnyShapeStyle(.red) : task.deadline == nil ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
                        if task.deadline != nil {
                            Button("Clear") { model.setDeadline(task, "") }
                                .buttonStyle(.link)
                        }
                    }
                }
                TextField("Deadline", text: $deadline, prompt: Text("Set deadline: fri, end of month…"))
                    .onSubmit {
                        model.setDeadline(task, deadline)
                        deadline = ""
                    }
                TaskInputField(
                    text: $schedule.text,
                    placeholder: "Change: fri 9am, every last fri, someday…",
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
            tags = task.tags.joined(separator: " ")
        }
        .onChange(of: task.tags) { _, synced in tags = synced.joined(separator: " ") }
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
        case .due: "Date"
        case .priority: "Priority"
        case .title: "Title"
        }
    }
}

extension TaskView: @retroactive Identifiable {}

/// The tag a view is about when it names just one (`#work`, or a smart list `#work !1`).
func soleTag(_ viewID: String) -> String? {
    if viewID.hasPrefix("#") { return String(viewID.dropFirst()) }
    return nil
}

let snoozes: [(String, Snooze)] = [
    ("Later Today", .laterToday), ("This Evening", .thisEvening), ("Tomorrow", .tomorrow),
    ("This Weekend", .weekend), ("Next Week", .nextWeek),
]


/// Catches a sideways two-finger swipe over its area (a scroll that's mostly horizontal, from the
/// trackpad), once per gesture: `onSwipe(true)` when the fingers move left, like turning a page.
private struct SwipeCatcher: NSViewRepresentable {
    let onSwipe: (Bool) -> Void

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        context.coordinator.view = view
        context.coordinator.install()
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) { context.coordinator.onSwipe = onSwipe }

    static func dismantleNSView(_ nsView: NSView, coordinator: Coordinator) { coordinator.remove() }

    func makeCoordinator() -> Coordinator { Coordinator(onSwipe: onSwipe) }

    @MainActor
    final class Coordinator {
        var onSwipe: (Bool) -> Void
        weak var view: NSView?
        private var monitor: Any?
        private var dx: CGFloat = 0
        private var dy: CGFloat = 0
        private var fired = false

        /// How far the fingers travel sideways before it counts, in points.
        private let threshold: CGFloat = 90

        init(onSwipe: @escaping (Bool) -> Void) { self.onSwipe = onSwipe }

        func install() {
            monitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
                self?.handle(event)
                return event
            }
        }

        func remove() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }

        private func handle(_ event: NSEvent) {
            // Trackpad gestures only (they have phases); a mouse wheel scrolls as usual.
            guard let view, event.window === view.window, event.hasPreciseScrollingDeltas else { return }
            if event.phase == .began {
                let point = view.convert(event.locationInWindow, from: nil)
                guard view.bounds.contains(point) else { fired = true; return }
                dx = 0; dy = 0; fired = false
            }
            guard event.phase == .changed, !fired else { return }
            dx += event.scrollingDeltaX
            dy += event.scrollingDeltaY
            if abs(dx) > threshold, abs(dx) > abs(dy) * 2 {
                fired = true
                // With natural scrolling, fingers moving left give a negative delta.
                let fingersLeft = event.isDirectionInvertedFromDevice ? dx < 0 : dx > 0
                onSwipe(fingersLeft)
            }
        }
    }
}


/// A day of Upcoming: its events and the tasks starting (or due) then.
struct UpcomingDay {
    let label: String
    var events: [CalendarEvent]
    var rows: [OutlineRow]
}

/// Upcoming's rows and the week's events by day: Tomorrow, then weekdays, then Later. A subtask
/// stays with its parent. Mirrors `byDay` on Android.
@MainActor
func upcomingDays(_ rows: [OutlineRow], events: [CalendarEvent], today: String) -> [UpcomingDay] {
    let start = AppModel.dayFormatter.date(from: today) ?? .now
    func label(_ day: String?) -> String {
        guard let day, let date = AppModel.dayFormatter.date(from: day),
              let days = Calendar.current.dateComponents([.day], from: start, to: date).day else { return "Later" }
        if days == 1 { return "Tomorrow" }
        if days < 7 { return date.formatted(.dateTime.weekday(.wide)) }
        return "Later"
    }
    let order = (1...6).map { label(AppModel.dayString(Calendar.current.date(byAdding: .day, value: $0, to: start)!)) } + ["Later"]
    var days = order.map { UpcomingDay(label: $0, events: [], rows: []) }
    func add(_ l: String, _ update: (inout UpcomingDay) -> Void) {
        if let i = days.firstIndex(where: { $0.label == l }) { update(&days[i]) }
    }
    var current = "Later"
    for row in rows {
        if row.depth == 0 {
            // Its start once it's ahead, else its deadline.
            let date = row.task.start.flatMap { $0 > today ? $0 : nil } ?? row.task.deadline
            current = label(date)
        }
        add(current) { $0.rows.append(row) }
    }
    for event in events where event.day > today { add(label(event.day)) { $0.events.append(event) } }
    return days.filter { !$0.rows.isEmpty || !$0.events.isEmpty }
}
