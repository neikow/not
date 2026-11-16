import AppKit
import Foundation
import NotCore
import Observation
import UniformTypeIdentifiers
import SystemConfiguration

@MainActor
@Observable
final class AppModel {
    let inboxID = inboxId()
    let todayID = todayViewId()
    let upcomingID = upcomingViewId()
    let somedayID = somedayViewId()
    /// Built-in views (Inbox, Today, Upcoming, Someday), then the user's smart lists.
    private(set) var lists: [TaskView] = []
    private(set) var tags: [TagInfo] = []
    /// The selected view, arranged for display.
    private(set) var outline = Outline(open: [], completed: [])
    /// Open tasks for today, for the Today badge.
    private(set) var dueCount = 0
    /// Tasks completed over a day ago, from every list, latest first (Settings → Archive).
    private(set) var archive: [TaskItem] = []
    /// Typed in the task list's search field; while set, the list shows matches from every list.
    var taskSearch = "" {
        didSet { if oldValue != taskSearch { reloadTasks() } }
    }
    var isSearching: Bool { !taskSearch.trimmingCharacters(in: .whitespaces).isEmpty }
    /// The selected task's subtasks.
    private var selectedSubtasks: [TaskItem] = []
    private(set) var syncState: SyncState?
    /// `nil` until this Mac is paired with a server.
    private(set) var syncInfo: SyncInfo?
    private(set) var canUndo = false
    private(set) var canRedo = false
    var errorMessage: String?

    var selection: SidebarItem? {
        didSet {
            guard oldValue != selection else { return }
            // Picking a list leaves search.
            if !taskSearch.isEmpty { taskSearch = "" }
            selectedTaskID = nil
            editingTaskID = nil
            selectedHabitID = nil
            selectedNoteID = nil
            addingSubtaskTo = nil
            reloadTasks()
            reloadNotes()
        }
    }

    /// The view shown, if the selection is one (`.list` holds any view id, or a `#tag`).
    var selectedListID: String? {
        get {
            switch selection {
            case .today: todayID
            case .upcoming: upcomingID
            case let .list(id): id
            default: nil
            }
        }
        set { selection = newValue.map(SidebarItem.list) }
    }
    var selectedTaskID: String? {
        didSet { if oldValue != selectedTaskID { loadSelectedTaskList() } }
    }
    /// The task whose title is being edited in place.
    var editingTaskID: String?
    /// The task getting a subtask typed in under it, in the list.
    var addingSubtaskTo: String?
    var showsInspector = false
    /// Bumped to move keyboard focus to the add-task field (⌘N).
    private(set) var addFieldFocusRequests = 0

    /// The list shown in the menu bar extra, remembered across launches.
    var panelListID: String = UserDefaults.standard.string(forKey: AppModel.panelListKey) ?? inboxId() {
        didSet {
            UserDefaults.standard.set(panelListID, forKey: Self.panelListKey)
            reloadPanel()
        }
    }
    /// Open tasks of `panelListID`, without completed subtasks.
    private(set) var panelRows: [OutlineRow] = []

    /// Habits as of `today`.
    private(set) var habits: [HabitSummary] = []
    /// When `habits` was computed: timed habits keep counting from there until the next reload.
    private(set) var habitsLoadedAt = Date()
    var selectedHabitID: String?
    /// The user's local date; habits are checked in on it.
    private(set) var today = AppModel.dayString(.now)

    private(set) var folders: [Folder] = []
    /// Notes of the selected folder (or all notes), or search results.
    private(set) var notes: [NoteSummary] = []
    private(set) var noteCount = 0
    var selectedNoteID: String?
    var noteSearch = "" {
        didSet { if oldValue != noteSearch { reloadNotes() } }
    }
    /// Bumped to move keyboard focus to the note editor.
    private(set) var editorFocusRequests = 0
    /// An image for the open note's editor to put in at the caret (see `MarkdownEditor`).
    private(set) var imageToInsert: (id: String, request: Int)?
    /// Bumped on every store change, so editors redraw images that just arrived.
    private(set) var imageVersion = 0

    /// Latest first.
    private(set) var workouts: [WorkoutSummary] = []
    var selectedWorkoutID: String? {
        didSet { if oldValue != selectedWorkoutID { reloadWorkout() } }
    }
    private(set) var workout: Workout?
    /// Exercise names used so far, most recent first.
    private(set) var exerciseNames: [String] = []
    /// Working sets per muscle since Monday.
    private(set) var weekMuscles: [MuscleSets] = []
    let restTimer = RestTimer()

    @ObservationIgnored private let store: NotStore
    @ObservationIgnored private var observer: ModelObserver?
    @ObservationIgnored private(set) lazy var reminders = Reminders(model: self)

    private static let panelListKey = "panelList"

    /// The computer name from System Settings, e.g. "Vitaly's MacBook Air".
    static var defaultDeviceName: String {
        (SCDynamicStoreCopyComputerName(nil, nil) as String?) ?? "Mac"
    }

    init() {
        var dir = URL.applicationSupportDirectory.appending(path: "dev.lysen.not", directoryHint: .isDirectory)
        #if DEBUG
        // Dev runs: `-dataDir /tmp/not-demo` keeps a separate, unpaired database.
        if let path = UserDefaults.standard.string(forKey: "dataDir") { dir = URL(filePath: path, directoryHint: .isDirectory) }
        #endif
        do {
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            store = try NotStore(path: dir.appending(path: "not.db").path)
        } catch {
            fatalError("Cannot open the local database in \(dir.path): \(error)")
        }
        let observer = ModelObserver(model: self)
        store.setObserver(observer: observer)
        self.observer = observer
        let store = store
        NoteImageCache.shared.load = { id in (try? store.image(id: id)).flatMap { $0 } }
        selectedListID = inboxID
        reload()

        let defaults = UserDefaults.standard
        // Before pairing, the shared server token was kept here in plain text.
        defaults.removeObject(forKey: "token")
        defaults.removeObject(forKey: "serverURL")

        syncInfo = run { try store.syncInfo() } ?? nil
        run { try store.startSync() }
        // Dev runs: `-serverURL http://127.0.0.1:8080 -pairingCode XXXXX-XXXXX` pairs on launch.
        if syncInfo == nil,
           let url = defaults.string(forKey: "serverURL"),
           let code = defaults.string(forKey: "pairingCode") {
            Task { await pair(serverURL: url, code: code, deviceName: Self.defaultDeviceName) }
        }
        #if DEBUG
        applyDevArguments(defaults)
        #endif
    }

    #if DEBUG
    /// Dev runs: `-seedDemo YES` fills an empty database with sample data (use with `-dataDir`);
    /// `-show habits|notes` opens that section, selecting the note titled `-note` or the first one.
    private func applyDevArguments(_ defaults: UserDefaults) {
        if defaults.bool(forKey: "seedDemo"), lists.count == 4, outline.open.isEmpty {
            for title in ["Buy oat milk today !2 #groceries", "Call the dentist tomorrow 9am", "Renew passport by fri !1", "Water plants every 2 days #home", "Learn the cello someday", "Draft report #work"] {
                quickAdd(title)
            }
            if let read = addHabit("Read") { toggleHabit(read) }
            _ = addHabit("Stretch")
            _ = addHabit("Water")
            let rule = TimedRule(atMostOff: true, usualOn: true, nudgeMinutes: 30)
            if let wear = run({ try store.createTimedHabit(title: "Wear retainer", minutes: 120, days: everyDay(), rule: rule) }) {
                let now = Self.nowMs()
                run { try store.setHabitState(id: wear, on: true, from: now - 5 * 3_600_000, to: nil, now: now) }
                run { try store.setHabitState(id: wear, on: false, from: now - 25 * 60_000, to: nil, now: now) }
            }
            for body in [
                "Trip to Lisbon\nPastéis de Belém, tram 28\n\n## Days\n1. Alfama\n2. Belém",
                "Pancakes\nFor Sunday breakfast\n- [ ] Flour\n- [x] Eggs\n- [ ] Milk\n\n- butter\n- **maple** syrup",
            ] {
                if let id = run({ try store.createNote(folderId: nil, body: body) }) { _ = id }
            }
            // A checklist made into tasks.
            if let note = run({ try store.createNote(folderId: nil, body: "Errands\nBefore leaving:\n- [ ] Post office\n- [x] Pharmacy\nThat's all") }) {
                let body = noteBody(note)
                if let moved = run({ try store.checklistToTasks(noteId: note, body: body, start: 0, end: UInt32((body as NSString).length), tag: "") }),
                   let tag = moved.tag,
                   let first = run({ try store.viewOutline(view: "#" + tag, today: today) })?.open.first {
                    _ = run { try store.addTask(target: first.task.id, title: "Stamps", today: today) }
                }
                _ = run { try store.createView(title: "Errands", query: "#errands") }
            }
            // A note with a picture (the app icon), to see images drawn in the editor.
            if let tiff = NSImage(named: NSImage.applicationIconName)?.tiffRepresentation,
               let png = NoteImageEncoder.encode(tiff), let id = addImage(png) {
                _ = run { try store.createNote(folderId: nil, body: "Photos\nThe icon:\n\(imageMarkup(id: id))\nafter") }
            }
        }
        switch defaults.string(forKey: "show") {
        case "habits": selection = .habits
        case "today": selection = .today
        case "workouts":
            selection = .workouts
            if workouts.isEmpty {
                _ = importWorkouts(Self.demoWorkoutLog)
                if let bench = searchExercises("Développé couché avec haltères", limit: 1).first {
                    linkExercise("Bench Haltères", to: bench.id)
                }
                reload()
                if let id = workouts.first?.id, let copy = run({ try store.repeatWorkout(fromId: id, date: today) }),
                   let first = run({ try store.workout(id: copy) })?.exercises.first {
                    updateExercise(first.id, name: first.name, sets: "E 18kg x 10\n26kg x 10 x 2\n26kg x 8\nfelt strong")
                }
                reload()
            }
            selectedWorkoutID = workouts.first?.id
        case "today": selection = .today
        case "notes":
            selection = .notes(nil)
            let title = defaults.string(forKey: "note")
            selectedNoteID = (notes.first { $0.title == title } ?? notes.first)?.id
        default: break
        }
    }

    private static let demoWorkoutLog = """
    30/10
    Tirage vertical (large)
    66kg x 8 x 3
    Straight Arm Pulldown (double)
    24.8kg x (10 x 2 + 8)
    Curl Biceps Haltères
    14kg x 10 x 2

    02/11
    Bench Haltères
    E 18kg x 10
    24kg x 10 x 3
    Shrugs
    24kg x 20
    24kg x 15 x 2
    Abdos
    """
    #endif

    var selectedList: TaskView? { lists.first { $0.id == selectedListID } }
    var isToday: Bool { selection == .today }
    var isUpcoming: Bool { selection == .upcoming }
    /// Search results and Upcoming (always by date) have no order of their own.
    var isDateView: Bool { isUpcoming || isSearching }
    var title: String {
        if isSearching { return "Search" }
        if let id = selectedListID, id.hasPrefix("#") { return id }
        return selectedList?.title ?? "not"
    }
    var inbox: TaskView? { lists.first { $0.id == inboxID } }
    /// Inbox, Today, Upcoming, Someday.
    var builtInViews: [TaskView] { lists.filter(\.builtIn) }
    /// The user's smart lists.
    var userLists: [TaskView] { lists.filter { !$0.builtIn } }
    var panelList: TaskView? { lists.first { $0.id == panelListID } }

    var selectedTask: TaskItem? {
        guard let id = selectedTaskID else { return nil }
        return (outline.open + outline.completed).first { $0.task.id == id }?.task
    }

    /// Today's open tasks planned for an earlier day: what "Move to today" picks up.
    var earlier: [TaskItem] {
        guard isToday else { return [] }
        return outline.open.filter { $0.depth == 0 && ($0.task.start.map { $0 < today } ?? false) }.map(\.task)
    }

    /// Whether the selected list still shows this task.
    func contains(_ taskID: String) -> Bool {
        (outline.open + outline.completed).contains { $0.task.id == taskID }
    }

    /// Direct subtasks of `taskID`, in order.
    func subtasks(of taskID: String) -> [TaskItem] {
        taskID == selectedTaskID ? selectedSubtasks : (run { try store.subtasks(id: taskID) } ?? [])
    }

    // MARK: Sync and pairing

    func syncNow() { store.syncNow() }

    fileprivate func didChangeSyncState(_ state: SyncState) { syncState = state }

    @discardableResult
    func pair(serverURL: String, code: String, deviceName: String) async -> Bool {
        let store = store
        guard let info = await runAsync({ try await store.pair(serverUrl: serverURL, code: code, deviceName: deviceName) })
        else { return false }
        syncInfo = info
        return true
    }

    func unpair() async {
        let store = store
        await runAsync { try await store.unpair() }
        syncInfo = nil
        syncState = nil
    }

    func createPairingCode() async -> PairingCode? {
        let store = store
        return await runAsync { try await store.createPairingCode() }
    }

    func devices() async -> [Device] {
        let store = store
        return await runAsync { try await store.devices() } ?? []
    }

    func shares(of listID: String) async -> [ListShare] {
        let store = store
        return await runAsync { try await store.shares(listId: listID) } ?? []
    }

    /// A new link (copied to the clipboard by the caller).
    func shareList(_ listID: String, canEdit: Bool) async -> ListShare? {
        let store = store
        return await runAsync { try await store.shareList(listId: listID, canEdit: canEdit) }
    }

    func revokeShare(_ share: ListShare) async {
        let store = store
        await runAsync { try await store.revokeShare(id: share.id) }
    }

    func revoke(_ device: Device) async {
        guard !device.isCurrent else { return await unpair() }
        let store = store
        await runAsync { try await store.revokeDevice(id: device.id) }
    }

    // MARK: Smart lists and tags

    /// Saves a smart list and shows it.
    @discardableResult
    func addList(_ title: String, query: String) -> String? {
        guard let id = run({ try store.createView(title: title, query: query) }) else { return nil }
        selectedListID = id
        return id
    }

    func updateList(_ id: String, title: String, query: String) {
        run { try store.updateView(id: id, title: title, query: query) }
    }

    func deleteList(_ id: String) {
        if selectedListID == id { selectedListID = inboxID }
        run { try store.deleteView(id: id) }
    }

    /// `index` among the user's smart lists.
    func moveList(_ id: String, to index: Int) {
        run { try store.moveView(id: id, index: UInt32(index)) }
    }

    /// `nil` when `query` is a valid filter, else what's wrong.
    func checkQuery(_ query: String) -> String? { store.checkQuery(query: query) }

    func countQuery(_ query: String) -> Int { Int(run { try store.countQuery(query: query, today: today) } ?? 0) }

    func renameTag(_ old: String, to new: String) {
        guard !new.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        run { try store.renameTag(old: old, new: new) }
        if selectedListID == "#" + old { selectedListID = "#" + normalizeTag(tag: new) }
    }

    func deleteTag(_ tag: String) {
        if selectedListID == "#" + tag { selectedListID = inboxID }
        run { try store.deleteTag(tag: tag) }
    }

    func setTags(_ id: String, _ tags: [String]) {
        run { try store.setTags(id: id, tags: tags) }
    }

    // MARK: Tasks

    func requestAddFieldFocus() { addFieldFocusRequests += 1 }

    func addTask(_ title: String) {
        addTask(title, to: selectedListID ?? inboxID)
    }

    /// `parentID` is a view (the task gets what it shows) or, for a subtask, a task.
    func addTask(_ title: String, to parentID: String) {
        guard !title.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        run { _ = try store.addTask(target: parentID, title: title, today: today) }
    }

    func rename(_ id: String, to title: String) {
        let title = title.trimmingCharacters(in: .whitespaces)
        guard !title.isEmpty else { return }
        run { try store.rename(id: id, title: title) }
    }

    func setNotes(_ id: String, _ notes: String) {
        run { try store.setNotes(id: id, notes: notes) }
    }

    func toggle(_ task: TaskItem) {
        refreshToday()
        run { try store.setCompleted(id: task.id, completed: task.completedAt == nil, today: today) }
    }

    /// What quick add would make of `text`, for highlighting and chips.
    func parse(_ text: String, dismissed: [String] = []) -> ParsedTask? {
        guard !text.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        return run { try store.parseTask(input: text, today: today, minute: Self.nowMinute, dismissed: dismissed) }
    }

    /// Minutes after midnight now, for "in 2 hours" and snoozing.
    static var nowMinute: UInt32 {
        let c = Calendar.current.dateComponents([.hour, .minute], from: .now)
        return UInt32((c.hour ?? 0) * 60 + (c.minute ?? 0))
    }

    /// Adds typed input with its dates and tags parsed out, to `parentID` or the selected view (which
    /// gives it its tags, Today's date…). Several lines (pasted) make a task each.
    func quickAdd(_ text: String, dismissed: [String] = [], to parentID: String? = nil) {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        refreshToday()
        let target = parentID ?? (isSearching ? inboxID : selectedListID ?? inboxID)
        if text.trimmingCharacters(in: .whitespacesAndNewlines).contains("\n") {
            run { _ = try store.quickAddLines(target: target, text: text, today: today, minute: Self.nowMinute) }
            return
        }
        run { _ = try store.quickAdd(target: target, input: text, today: today, minute: Self.nowMinute, dismissed: dismissed) }
    }

    /// Takes the start date, time, deadline, Someday and repeat out of `text` ("fri 9am by mon every
    /// week"); blank clears them.
    func setSchedule(_ id: String, _ text: String) {
        guard let p = parse(text) else {
            run { try store.setSchedule(id: id, start: nil, time: nil, deadline: nil, someday: false, repeat: nil) }
            return
        }
        run { try store.setSchedule(id: id, start: p.start, time: p.time, deadline: p.deadline, someday: p.someday, repeat: p.repeat) }
    }

    /// Only the deadline, from words ("fri", "end of month"); blank clears it.
    func setDeadline(_ task: TaskItem, _ text: String) {
        var deadline: String?
        if !text.trimmingCharacters(in: .whitespaces).isEmpty {
            guard let p = parse(text), let day = p.start ?? p.deadline else { return }
            deadline = day
        }
        run { try store.setSchedule(id: task.id, start: task.start, time: task.time, deadline: deadline, someday: task.someday, repeat: task.repeat) }
    }

    func setSomeday(_ task: TaskItem) {
        run { try store.setSchedule(id: task.id, start: nil, time: nil, deadline: task.deadline, someday: true, repeat: task.repeat) }
    }

    func snooze(_ ids: [String], _ to: Snooze) {
        refreshToday()
        run { _ = try store.snooze(ids: ids, to: to, today: today, minute: Self.nowMinute) }
    }

    /// What slipped past its day comes to today, in one undo step.
    func moveToToday(_ ids: [String]) {
        guard !ids.isEmpty else { return }
        refreshToday()
        run { try store.reschedule(ids: ids, start: today) }
    }

    /// "Still relevant" in the weekly review.
    func keep(_ id: String) { run { try store.keepTask(id: id) } }

    /// Open top-level tasks a view shows now (for the review).
    func viewTasks(_ id: String) -> [TaskItem] {
        (run { try store.viewOutline(view: id, today: today) }?.open ?? []).filter { $0.depth == 0 }.map(\.task)
    }

    func staleTasks() -> [TaskItem] { run { try store.staleTasks(days: 30) } ?? [] }

    func logbook() -> [TaskItem] { run { try store.logbook(limit: 300) } ?? [] }

    func setPriority(_ id: String, _ priority: UInt8) {
        run { try store.setPriority(id: id, priority: priority) }
    }

    func setSort(_ listID: String, _ sort: TaskSort) {
        run { try store.setViewSort(id: listID, sort: sort) }
    }

    func toggleSelected() {
        if let task = selectedTask { toggle(task) }
    }

    func delete(id: String) {
        if selectedTaskID == id { selectedTaskID = nil }
        run { try store.deleteItem(id: id) }
    }

    func deleteSelected() {
        if let id = selectedTaskID { delete(id: id) }
    }

    /// Drag and drop within the open tasks: `destination` is an index into `outline.open` (before
    /// the move). The task stays under its parent, placed before the next sibling at the drop point.
    func moveOpenRow(from source: Int, to destination: Int) {
        let rows = outline.open
        let moving = rows[source]
        let anchor = rows[destination...].first {
            $0.depth == moving.depth && (moving.depth == 0 || $0.task.parentId == moving.task.parentId) && $0.task.id != moving.task.id
        }
        place(moving.task, before: anchor?.task.id)
    }

    /// ⌘↑ / ⌘↓: swaps the selected task with its previous or next open sibling.
    func moveSelected(by offset: Int) {
        guard let task = selectedTask, task.completedAt == nil else { return }
        let depth = outline.open.first { $0.task.id == task.id }?.depth ?? 0
        let siblings = outline.open.filter { $0.depth == depth && (depth == 0 || $0.task.parentId == task.parentId) }.map(\.task)
        guard let index = siblings.firstIndex(where: { $0.id == task.id }) else { return }
        let target = index + offset
        guard siblings.indices.contains(target) else { return }
        // Moving down means landing before the sibling after the next one.
        let anchorIndex = offset < 0 ? target : target + 1
        place(task, before: siblings.indices.contains(anchorIndex) ? siblings[anchorIndex].id : nil)
    }

    /// Opens the inline field for a subtask under `taskID` (the selected task by default).
    func beginSubtask(of taskID: String? = nil) {
        guard let id = taskID ?? selectedTaskID, !isDateView,
              outline.open.contains(where: { $0.task.id == id })
        else { return }
        editingTaskID = nil
        addingSubtaskTo = id
    }

    /// Tab / ⌘]: the selected task becomes a subtask of the one above it.
    func indentSelected() {
        guard let id = selectedTaskID, !isDateView, let view = selectedListID else { return }
        run { _ = try store.indentInView(view: view, id: id, today: today) }
    }

    /// ⇧Tab / ⌘[: the selected subtask moves out a level.
    func outdentSelected() {
        guard let id = selectedTaskID, !isDateView, let view = selectedListID else { return }
        run { _ = try store.outdentInView(view: view, id: id) }
    }

    private func place(_ task: TaskItem, before anchor: String?) {
        let atTop = outline.open.contains { $0.task.id == task.id && $0.depth == 0 }
        if atTop, let view = selectedListID {
            run { try store.moveInView(view: view, id: task.id, beforeId: anchor, today: today) }
        } else {
            run { try store.moveTaskBefore(id: task.id, parentId: task.parentId, beforeId: anchor) }
        }
    }

    // MARK: Habits

    var selectedHabit: HabitSummary? { habits.first { $0.habit.id == selectedHabitID } }

    /// The habits on today's to-do, done ones included, for the top of Today.
    var todayHabits: [HabitSummary] { habits.filter(\.onToday) }

    /// Shows a habit in Habits, with its inspector open (from Today).
    func openHabit(_ id: String) {
        selection = .habits
        selectedHabitID = id
        showsInspector = true
    }

    @discardableResult
    func addHabit(_ title: String) -> String? {
        guard !title.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        return run { try store.createHabit(title: title, goal: 1, days: everyDay()) }
    }

    func updateHabit(_ habit: Habit) {
        run {
            if let rule = habit.timed {
                try store.updateTimedHabit(id: habit.id, title: habit.title, minutes: habit.goal, days: habit.days, rule: rule)
            } else {
                try store.updateHabit(id: habit.id, title: habit.title, goal: habit.goal, days: habit.days, period: habit.period)
            }
        }
    }

    /// A check-in until the goal is met, then back to zero.
    func toggleHabit(_ id: String) {
        run { _ = try store.toggleHabit(id: id, date: today) }
    }

    /// A timed habit: on (or off) from now.
    func switchHabit(_ id: String, on: Bool) {
        let now = Self.nowMs()
        run { try store.setHabitState(id: id, on: on, from: now, to: nil, now: now) }
    }

    static func nowMs() -> Int64 { Int64(Date().timeIntervalSince1970 * 1000) }

    /// Minutes east of UTC right now (UTC+1 = 60), how the core places local days.
    static func utcOffsetMinutes() -> Int32 { Int32(TimeZone.current.secondsFromGMT() / 60) }

    func logHabit(_ id: String, _ delta: Int64) {
        run { _ = try store.logHabit(id: id, date: today, delta: delta) }
    }

    func deleteHabit(_ id: String) {
        if selectedHabitID == id { selectedHabitID = nil }
        run { try store.deleteHabit(id: id) }
    }

    func moveHabit(from source: Int, to destination: Int) {
        // `destination` counts the moved row itself when moving down.
        let index = source < destination ? destination - 1 : destination
        run { try store.moveHabit(id: habits[source].habit.id, index: UInt32(index)) }
    }

    /// Check-ins per day (`YYYY-MM-DD`) from `from` through today.
    func habitHistory(_ id: String, from: Date) -> [String: UInt32] {
        let days = run { try store.habitDays(id: id, from: Self.dayString(from), to: today, nowMs: Self.nowMs(), utcOffsetMinutes: Self.utcOffsetMinutes()) } ?? []
        return Dictionary(uniqueKeysWithValues: days.map { ($0.date, $0.count) })
    }

    /// Picks up a new day (midnight, or waking up the next morning).
    func refreshToday() {
        let now = Self.dayString(.now)
        guard now != today else { return }
        today = now
        reloadHabits()
        reloadTasks()
    }

    /// Parses `YYYY-MM-DD` as local midnight.
    static let dayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.calendar = Calendar(identifier: .gregorian)
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd"
        return f
    }()

    // ---- export ----

    /// Asks where, then writes everything as readable JSON.
    func exportJSON() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "not-export-\(today).json"
        panel.allowedContentTypes = [.json]
        guard panel.runModal() == .OK, let url = panel.url,
              let json = run({ try store.exportJson(today: today) }) else { return }
        do { try Data(json.utf8).write(to: url) } catch { errorMessage = error.localizedDescription }
    }

    func saveBackup() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "not-backup-\(today).notbackup"
        guard panel.runModal() == .OK, let url = panel.url, let bytes = run({ try store.backup() }) else { return }
        do { try bytes.write(to: url) } catch { errorMessage = error.localizedDescription }
    }

    /// Merges a backup file in, like a sync.
    func restoreBackup() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            let bytes = try Data(contentsOf: url)
            run { try store.restoreBackup(bytes: bytes) }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    // ---- reminders ----

    /// Open tasks starting at a time: what reminders are set for.
    func reminderTasks() -> [TaskItem] {
        run { try store.timedTasks() } ?? []
    }

    /// Open tasks for `day` (Today as of then), for the morning summary.
    func reminderDue(on day: String) -> [TaskItem] {
        (run { try store.viewOutline(view: todayID, today: day) }?.open ?? []).filter { $0.depth == 0 }.map(\.task)
    }

    /// Titles of habits due on `day` and not done yet.
    func reminderHabitsLeft(on day: String) -> [String] {
        (run { try store.habits(date: day, nowMs: Self.nowMs(), utcOffsetMinutes: Self.utcOffsetMinutes()) } ?? []).filter { $0.due && !$0.done }.map(\.habit.title)
    }

    /// From a notification's Done button.
    func complete(taskID: String) {
        run { try store.setCompleted(id: taskID, completed: true, today: today) }
    }

    /// From clicking a notification: the task in its list, selected.
    func reveal(taskID: String) {
        guard run({ try store.taskById(id: taskID) }) != nil else { return }
        selection = .today
        selectedTaskID = taskID
        NSApp.activate()
    }

    static func dayString(_ date: Date) -> String {
        let c = Calendar.current.dateComponents([.year, .month, .day], from: date)
        return String(format: "%04d-%02d-%02d", c.year ?? 0, c.month ?? 0, c.day ?? 0)
    }

    // MARK: Notes

    /// The folder shown, if any (`nil` for All Notes too).
    var selectedFolderID: String? {
        if case let .notes(id) = selection { id } else { nil }
    }

    var selectedNote: NoteSummary? { notes.first { $0.id == selectedNoteID } }

    func folderTitle(_ id: String?) -> String {
        id.flatMap { id in folders.first { $0.id == id }?.title } ?? "Notes"
    }

    func newNote() {
        noteSearch = ""
        guard let id = run({ try store.createNote(folderId: selectedFolderID, body: "") }) else { return }
        selectedNoteID = id
        editorFocusRequests += 1
    }

    func noteBody(_ id: String) -> String {
        run { try store.noteBody(id: id) } ?? ""
    }

    func setNoteBody(_ id: String, _ body: String) {
        run { try store.setNoteBody(id: id, body: body) }
    }

    /// The body to open the editor with (notes from earlier versions brought up to date).
    func openNote(_ id: String) -> String {
        run { try store.openNote(id: id) } ?? ""
    }



    /// Keeps image data (downscaled first) for notes; its id, for `imageMarkup`.
    func addImage(_ data: Data) -> String? {
        run { try store.addImage(bytes: data) }
    }

    /// Picks image files and puts them into the open note at the caret.
    func pickImagesForNote() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = true
        panel.message = "Add images to the note"
        guard panel.runModal() == .OK else { return }
        for url in panel.urls {
            guard let data = try? Data(contentsOf: url), let encoded = NoteImageEncoder.encode(data), let id = addImage(encoded) else { continue }
            imageToInsert = (id, (imageToInsert?.request ?? 0) + 1)
        }
    }

    /// Makes tasks of the checklist items `start..<end` (UTF-16) touches in `body` (saved with
    /// them), tagged `tag` ("" for one from the heading), or untagged (Inbox) when nil. The lines
    /// stay as they are; the tasks are a copy.
    func checklistToTasks(_ noteID: String, _ body: String, start: Int, end: Int, tag: String?) -> Bool {
        run {
            try store.checklistToTasks(noteId: noteID, body: body, start: UInt32(start), end: UInt32(end), tag: tag)
        } != nil
    }

    func togglePin(_ note: NoteSummary) {
        run { try store.setNotePinned(id: note.id, pinned: !note.pinned) }
    }

    func moveNote(_ id: String, to folderID: String?) {
        run { try store.moveNote(id: id, folderId: folderID) }
    }

    /// The selected folder sorts its notes by hand.
    var isManualOrder: Bool {
        folders.first { $0.id == selectedFolderID }?.manualOrder ?? false
    }

    func setManualOrder(_ manual: Bool) {
        guard let folder = selectedFolderID else { return }
        run { try store.setFolderManualOrder(id: folder, manual: manual) }
    }

    /// Notes dropped into the shown folder's list at `index` (of `notes`). Notes from elsewhere join
    /// the folder first. The folder switches to manual order.
    func dropNotes(_ ids: [String], at index: Int) {
        guard let folder = selectedFolderID, noteSearch.isEmpty else { return }
        for id in ids {
            guard let note = notes.first(where: { $0.id == id }) ?? searchAll(id) else { continue }
            if note.folderId != folder { run { try store.moveNote(id: id, folderId: folder) } }
            // Pinned notes stay above the others, so anchor within the same group.
            let anchor = notes[min(index, notes.count)...].first { $0.id != id && $0.pinned == note.pinned }
            run { try store.moveNoteBefore(id: id, beforeId: anchor?.id) }
        }
    }

    private func searchAll(_ id: String) -> NoteSummary? {
        (run { try store.notes(folderId: nil) } ?? []).first { $0.id == id }
    }

    func deleteNote(_ id: String) {
        if selectedNoteID == id { selectedNoteID = nil }
        run { try store.deleteNote(id: id) }
    }

    /// Leaving a note nobody typed into removes it, like Notes does.
    func discardIfEmpty(_ id: String) {
        guard notes.contains(where: { $0.id == id }),
              noteBody(id).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else { return }
        run { try store.deleteNote(id: id) }
    }

    @discardableResult
    func addFolder(in parentID: String? = nil) -> String? {
        guard let id = run({ try store.createFolder(parentId: parentID, title: "New Folder") }) else { return nil }
        selection = .notes(id)
        return id
    }

    func renameFolder(_ id: String, to title: String) {
        let title = title.trimmingCharacters(in: .whitespaces)
        guard !title.isEmpty else { return }
        run { try store.renameFolder(id: id, title: title) }
    }

    func moveFolder(_ id: String, into parentID: String?) {
        run { try store.moveFolder(id: id, parentId: parentID) }
    }

    func deleteFolder(_ id: String) {
        if selectedFolderID == id { selection = .notes(nil) }
        run { try store.deleteNote(id: id) }
    }

    /// ⌘N: a new task, habit, note or workout, depending on what is shown.
    func newItem() {
        switch selection {
        case .notes: newNote()
        case .workouts: newWorkout()
        default: requestAddFieldFocus()
        }
    }

    // MARK: Workouts

    /// A session for today, or the one already started today.
    func newWorkout(title: String = "") {
        refreshToday()
        guard let id = run({ try store.createWorkout(date: today, title: title) }) else { return }
        selectedWorkoutID = id
    }

    func repeatWorkout(_ id: String) {
        refreshToday()
        guard let copy = run({ try store.repeatWorkout(fromId: id, date: today) }) else { return }
        selectedWorkoutID = copy
    }

    func updateWorkout(_ id: String, date: String, title: String) {
        run { try store.updateWorkout(id: id, date: date, title: title) }
    }

    func deleteWorkout(_ id: String) {
        if selectedWorkoutID == id { selectedWorkoutID = nil }
        run { try store.deleteWorkout(id: id) }
    }

    @discardableResult
    func addExercise(_ name: String, to workoutID: String) -> String? {
        guard !name.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        return run { try store.addExercise(workoutId: workoutID, name: name, sets: "") }
    }

    func updateExercise(_ id: String, name: String, sets: String) {
        run { try store.updateExercise(id: id, name: name, sets: sets) }
    }

    func setExerciseDone(_ id: String, _ done: Bool) {
        run { try store.setExerciseDone(id: id, done: done) }
    }

    func deleteExercise(_ id: String) {
        run { try store.deleteExercise(id: id) }
    }

    func moveExercise(_ id: String, to index: Int) {
        run { try store.moveExercise(id: id, index: UInt32(max(index, 0))) }
    }

    func exerciseHistory(_ name: String) -> [ExerciseLog] {
        run { try store.exerciseHistory(name: name, limit: 30) } ?? []
    }

    func searchExercises(_ query: String, limit: Int = 8) -> [CatalogExercise] {
        store.searchExercises(query: query, limit: UInt32(limit))
    }

    func exerciseInfo(_ name: String) -> CatalogExercise? {
        // Read so views showing a link redraw when it changes; the store itself isn't observed.
        _ = linksRevision
        return store.exerciseInfo(name: name)
    }

    /// Bumped by each link change, so views calling ``exerciseInfo(_:)`` redraw.
    private(set) var linksRevision = 0

    /// `nil` unlinks the name from the catalogue.
    func linkExercise(_ name: String, to catalogID: String?) {
        run { try store.linkExercise(name: name, catalogId: catalogID) }
        linksRevision += 1
    }

    /// Adds sessions from a pasted text log; returns how many.
    func importWorkouts(_ text: String) -> Int {
        refreshToday()
        return Int(run { try store.importWorkouts(text: text, today: today) } ?? 0)
    }

    // MARK: Undo

    func undo() { run { _ = try store.undo() } }

    /// Puts an archived task back in its list, open.
    func restore(_ task: TaskItem) {
        run { try store.setCompleted(id: task.id, completed: false, today: today) }
    }

    func clearArchive() {
        run { _ = try store.clearArchive() }
    }
    func redo() { run { _ = try store.redo() } }

    /// `not://add?title=Milk[&list=Groceries]` — the smart list is matched by title, else the
    /// Inbox; `#tags` in the title work as in quick add.
    func handle(url: URL) {
        guard url.scheme == "not", url.host() == "add",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let title = items.first(where: { $0.name == "title" })?.value
        else { return }
        let listName = items.first { $0.name == "list" }?.value ?? ""
        let target = lists.first { $0.title.caseInsensitiveCompare(listName) == .orderedSame }?.id ?? inboxID
        quickAdd(title, to: target)
    }

    // MARK: Reading

    fileprivate func reload() {
        imageVersion += 1
        reminders.reschedule()
        guard let lists = run({ try store.views(today: today) }) else { return }
        self.lists = lists
        canUndo = store.canUndo()
        canRedo = store.canRedo()
        tags = run { try store.tags() } ?? []
        dueCount = Int(lists.first { $0.id == todayID }?.openCount ?? 0)
        switch selection {
        case .today, .upcoming:
            reloadTasks()
        case .habits, .workouts:
            break
        case .notes:
            break // reloadNotes() handles deleted folders.
        case let .list(id) where lists.contains(where: { $0.id == id }) || tags.contains(where: { "#" + $0.name == id }):
            reloadTasks()
        default:
            selectedListID = inboxID
        }
        reloadPanel()
        reloadHabits()
        reloadNotes()
        reloadWorkouts()
    }

    private func reloadWorkouts() {
        workouts = run { try store.workouts() } ?? []
        exerciseNames = run { try store.exerciseNames() } ?? []
        if let now = parseDay(today) {
            let weekday = (Calendar.current.component(.weekday, from: now) + 5) % 7 // Monday = 0
            let monday = Calendar.current.date(byAdding: .day, value: -weekday, to: now) ?? now
            weekMuscles = run { try store.muscleSets(from: Self.dayString(monday), to: today) } ?? []
        }
        if let id = selectedWorkoutID, !workouts.contains(where: { $0.id == id }) {
            selectedWorkoutID = nil
        }
        reloadWorkout()
    }

    private func reloadWorkout() {
        workout = selectedWorkoutID.flatMap { id in run { try store.workout(id: id) } }
    }

    private func reloadNotes() {
        folders = run { try store.folders() } ?? []
        if case let .notes(id?) = selection, !folders.contains(where: { $0.id == id }) {
            // The folder was deleted, possibly on another device.
            selection = .notes(nil)
            return
        }
        guard case let .notes(folder) = selection else {
            noteCount = (run { try store.notes(folderId: nil) } ?? []).count
            return
        }
        let query = noteSearch.trimmingCharacters(in: .whitespaces)
        notes = (query.isEmpty ? run { try store.notes(folderId: folder) } : run { try store.searchNotes(query: query) }) ?? []
        noteCount = folder == nil && query.isEmpty ? notes.count : (run { try store.notes(folderId: nil) } ?? []).count
        if selectedNote == nil { selectedNoteID = nil }
    }

    private func reloadHabits() {
        habits = run { try store.habits(date: today, nowMs: Self.nowMs(), utcOffsetMinutes: Self.utcOffsetMinutes()) } ?? []
        habitsLoadedAt = Date()
        if selectedHabit == nil { selectedHabitID = nil }
    }

    private func reloadPanel() {
        // The list may have been deleted on another device.
        guard lists.contains(where: { $0.id == panelListID }) else {
            panelListID = inboxID
            return
        }
        panelRows = (run { try store.viewOutline(view: panelListID, today: today) })?.open.filter { !$0.done } ?? []
    }

    private func reloadTasks() {
        archive = run { try store.archive() } ?? []
        if isSearching {
            let found = run { try store.searchTasks(query: taskSearch, limit: 100) } ?? []
            outline = Outline(
                open: found.filter { $0.completedAt == nil }.map { OutlineRow(task: $0, depth: 0, done: false, subtasks: 0, subtasksDone: 0) },
                completed: found.filter { $0.completedAt != nil }.map { OutlineRow(task: $0, depth: 0, done: true, subtasks: 0, subtasksDone: 0) }
            )
        } else if let list = selectedListID, let outline = run({ try store.viewOutline(view: list, today: today) }) {
            self.outline = outline
        } else {
            outline = Outline(open: [], completed: [])
            return
        }
        loadSelectedTaskList()
        if let id = selectedTaskID, selectedTask == nil {
            // Deleted or moved away, possibly on another device.
            selectedTaskID = nil
            if editingTaskID == id { editingTaskID = nil }
        }
        if let id = addingSubtaskTo, !outline.open.contains(where: { $0.task.id == id }) {
            addingSubtaskTo = nil
        }
    }

    private func loadSelectedTaskList() {
        selectedSubtasks = selectedTaskID.flatMap { id in run { try store.subtasks(id: id) } } ?? []
    }

    @discardableResult
    private func run<T>(_ body: () throws -> T) -> T? {
        do {
            return try body()
        } catch {
            errorMessage = Self.describe(error)
            return nil
        }
    }

    @discardableResult
    private func runAsync<T: Sendable>(_ body: @Sendable () async throws -> T) async -> T? {
        do {
            return try await body()
        } catch {
            errorMessage = Self.describe(error)
            return nil
        }
    }

    private static func describe(_ error: Error) -> String {
        switch error as? NotError {
        case let .NotFound(detail), let .Invalid(detail), let .Internal(detail): detail
        case nil: error.localizedDescription
        }
    }
}

/// What the sidebar shows in the main column.
enum SidebarItem: Hashable {
    /// The Today view.
    case today
    /// The Upcoming view.
    case upcoming
    /// Any other view (Inbox, Someday, a smart list) or a `#tag`.
    case list(String)
    case habits
    case workouts
    /// A notes folder, or all notes for `nil`.
    case notes(String?)
}

/// Bridges Rust callbacks (background threads) onto the main actor.
private final class ModelObserver: StoreObserver, @unchecked Sendable {
    private weak var model: AppModel?

    init(model: AppModel) { self.model = model }

    func onChange() {
        Task { @MainActor [weak self] in self?.model?.reload() }
    }

    func onSyncState(state: SyncState) {
        Task { @MainActor [weak self] in self?.model?.didChangeSyncState(state) }
    }
}
