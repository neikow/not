import Foundation
import NotCore
import Observation
import SystemConfiguration

@MainActor
@Observable
final class AppModel {
    let inboxID = inboxId()
    private(set) var lists: [TaskList] = []
    /// The selected list, arranged for display.
    private(set) var outline = Outline(open: [], completed: [])
    private(set) var syncState: SyncState?
    /// `nil` until this Mac is paired with a server.
    private(set) var syncInfo: SyncInfo?
    private(set) var canUndo = false
    private(set) var canRedo = false
    var errorMessage: String?

    var selectedListID: String? {
        didSet {
            guard oldValue != selectedListID else { return }
            selectedTaskID = nil
            editingTaskID = nil
            reloadTasks()
        }
    }
    var selectedTaskID: String?
    /// The task whose title is being edited in place.
    var editingTaskID: String?
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

    @ObservationIgnored private let store: NotStore
    @ObservationIgnored private var observer: ModelObserver?

    private static let panelListKey = "panelList"

    /// The computer name from System Settings, e.g. "Vitaly's MacBook Air".
    static var defaultDeviceName: String {
        (SCDynamicStoreCopyComputerName(nil, nil) as String?) ?? "Mac"
    }

    init() {
        let dir = URL.applicationSupportDirectory.appending(path: "dev.lysen.not", directoryHint: .isDirectory)
        do {
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            store = try NotStore(path: dir.appending(path: "not.db").path)
        } catch {
            fatalError("Cannot open the local database in \(dir.path): \(error)")
        }
        let observer = ModelObserver(model: self)
        store.setObserver(observer: observer)
        self.observer = observer
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
    }

    var selectedList: TaskList? { lists.first { $0.id == selectedListID } }
    var inbox: TaskList? { lists.first { $0.id == inboxID } }
    var userLists: [TaskList] { lists.filter { $0.id != inboxID } }
    var panelList: TaskList? { lists.first { $0.id == panelListID } }

    var selectedTask: TaskItem? {
        guard let id = selectedTaskID else { return nil }
        return (outline.open + outline.completed).first { $0.task.id == id }?.task
    }

    /// Whether the selected list still shows this task.
    func contains(_ taskID: String) -> Bool {
        (outline.open + outline.completed).contains { $0.task.id == taskID }
    }

    /// Direct subtasks of `taskID` in the selected list, in order.
    func subtasks(of taskID: String) -> [TaskItem] {
        (outline.open + outline.completed).map(\.task).filter { $0.parentId == taskID }
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

    func revoke(_ device: Device) async {
        guard !device.isCurrent else { return await unpair() }
        let store = store
        await runAsync { try await store.revokeDevice(id: device.id) }
    }

    // MARK: Lists

    /// Creates a list and returns its id, so the caller can start renaming it.
    @discardableResult
    func addList(_ title: String) -> String? {
        guard let id = run({ try store.createList(title: title) }) else { return nil }
        selectedListID = id
        return id
    }

    func moveList(_ id: String, to index: Int) {
        run { try store.moveList(id: id, index: UInt32(index)) }
    }

    // MARK: Tasks

    func requestAddFieldFocus() { addFieldFocusRequests += 1 }

    func addTask(_ title: String) {
        addTask(title, to: selectedListID ?? inboxID)
    }

    /// `parentID` is a list or, for a subtask, a task.
    func addTask(_ title: String, to parentID: String) {
        guard !title.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        run { _ = try store.addTask(parentId: parentID, title: title) }
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
        run { try store.setCompleted(id: task.id, completed: task.completedAt == nil) }
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

    /// Moves a task (with its subtasks) to the end of another list.
    func move(_ taskID: String, toList listID: String) {
        run { try store.moveTask(id: taskID, parentId: listID, index: .max) }
    }

    /// Drag and drop within the open tasks: `destination` is an index into `outline.open` (before
    /// the move). The task stays under its parent, placed before the next sibling at the drop point.
    func moveOpenRow(from source: Int, to destination: Int) {
        let rows = outline.open
        let moving = rows[source].task
        let anchor = rows[destination...].first { $0.task.parentId == moving.parentId && $0.task.id != moving.id }
        place(moving, before: anchor?.task.id)
    }

    /// ⌘↑ / ⌘↓: swaps the selected task with its previous or next open sibling.
    func moveSelected(by offset: Int) {
        guard let task = selectedTask, task.completedAt == nil else { return }
        let siblings = outline.open.map(\.task).filter { $0.parentId == task.parentId }
        guard let index = siblings.firstIndex(where: { $0.id == task.id }) else { return }
        let target = index + offset
        guard siblings.indices.contains(target) else { return }
        // Moving down means landing before the sibling after the next one.
        let anchorIndex = offset < 0 ? target : target + 1
        place(task, before: siblings.indices.contains(anchorIndex) ? siblings[anchorIndex].id : nil)
    }

    private func place(_ task: TaskItem, before anchor: String?) {
        let parent = task.parentId ?? task.listId
        run { try store.moveTaskBefore(id: task.id, parentId: parent, beforeId: anchor) }
    }

    // MARK: Undo

    func undo() { run { _ = try store.undo() } }
    func redo() { run { _ = try store.redo() } }

    /// `not://add?title=Milk[&list=Groceries]` — the list is matched by title, else the Inbox.
    func handle(url: URL) {
        guard url.scheme == "not", url.host() == "add",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let title = items.first(where: { $0.name == "title" })?.value
        else { return }
        let listName = items.first { $0.name == "list" }?.value ?? ""
        let target = lists.first { $0.title.caseInsensitiveCompare(listName) == .orderedSame }?.id ?? inboxID
        addTask(title, to: target)
    }

    // MARK: Reading

    fileprivate func reload() {
        guard let lists = run({ try store.lists() }) else { return }
        self.lists = lists
        canUndo = store.canUndo()
        canRedo = store.canRedo()
        if let selected = selectedListID, lists.contains(where: { $0.id == selected }) {
            reloadTasks()
        } else {
            selectedListID = inboxID
        }
        reloadPanel()
    }

    private func reloadPanel() {
        // The list may have been deleted on another device.
        guard lists.contains(where: { $0.id == panelListID }) else {
            panelListID = inboxID
            return
        }
        panelRows = (run { try store.outline(listId: panelListID) })?.open.filter { !$0.done } ?? []
    }

    private func reloadTasks() {
        guard let list = selectedListID, let outline = run({ try store.outline(listId: list) }) else {
            outline = Outline(open: [], completed: [])
            return
        }
        self.outline = outline
        if let id = selectedTaskID, selectedTask == nil {
            // Deleted or moved away, possibly on another device.
            selectedTaskID = nil
            if editingTaskID == id { editingTaskID = nil }
        }
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
