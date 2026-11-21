import CoreImage.CIFilterBuiltins
import NotCore
import SwiftUI

struct SettingsView: View {
    let model: AppModel

    var body: some View {
        TabView {
            Tab("General", systemImage: "gearshape") {
                GeneralSettings(model: model)
            }
            Tab("Calendar", systemImage: "calendar") {
                CalendarSettings(overlay: model.calendar)
            }
            Tab("Trash", systemImage: "trash") {
                TrashSettings(model: model)
            }
            Tab("Credits", systemImage: "heart") {
                Form {
                    Section("Exercise data") {
                        Text("Exercise names, muscles and equipment come from the free exercise database of [wger](https://wger.de), shared by its contributors under [CC-BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/), CC-BY-SA 3.0 or CC0. Each exercise credits its author in its history.")
                            .foregroundStyle(.secondary)
                    }
                }
            }
            Tab("Sync", systemImage: "arrow.triangle.2.circlepath.icloud") {
                Group {
                    if let info = model.syncInfo {
                        PairedView(model: model, info: info)
                    } else {
                        PairForm(model: model)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 460)
    }
}

/// What was deleted, newest first; it goes for good after 30 days.
private struct TrashSettings: View {
    let model: AppModel

    var body: some View {
        // Bumped on every store change, so the list follows deletes made anywhere.
        let _ = model.imageVersion
        let items = model.trash
        Form {
            Section {
                if items.isEmpty {
                    Text("Deleted tasks, notes, habits and workouts wait here for \(trashDays()) days.")
                        .foregroundStyle(.secondary)
                }
                ForEach(items, id: \.id) { item in
                    HStack {
                        Image(systemName: icon(item.kind)).foregroundStyle(.secondary).frame(width: 18)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(item.title.isEmpty ? "Untitled" : item.title)
                            Text(detail(item)).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Put Back") { model.restoreFromTrash(item.id) }
                        Button(role: .destructive) { model.deleteForever(item.id) } label: { Image(systemName: "trash.slash") }
                            .buttonStyle(.borderless)
                            .help("Delete for good (⌘Z undoes it until you quit)")
                    }
                }
            } footer: {
                if !items.isEmpty {
                    HStack {
                        Text("Items go for good after \(trashDays()) days.").font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Button("Empty Trash", role: .destructive) { model.emptyTrash() }
                    }
                }
            }
        }
        .frame(minHeight: 320)
    }

    private func icon(_ kind: TrashKind) -> String {
        switch kind {
        case .task: "checkmark.circle"
        case .note: "note.text"
        case .folder: "folder"
        case .habit: "repeat"
        case .workout, .exercise: "dumbbell"
        }
    }

    private func detail(_ item: TrashItem) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(item.deletedAt) / 1000)
        let kind = switch item.kind {
        case .task: "Task"
        case .note: "Note"
        case .folder: "Folder"
        case .habit: "Habit"
        case .workout: "Workout"
        case .exercise: "Exercise"
        }
        let inside = item.inside > 0 ? "with \(item.inside) inside" : nil
        return [kind, "deleted " + date.formatted(.relative(presentation: .named)), inside].compactMap { $0 }.joined(separator: " · ")
    }
}

private struct GeneralSettings: View {
    let model: AppModel
    @AppStorage(MenuBarPanel.visibleKey) private var showsMenuBarExtra = true

    var body: some View {
        Form {
            Section {
                Toggle("Show in menu bar", isOn: $showsMenuBarExtra)
            } footer: {
                Text("Add and tick off tasks from the menu bar without switching apps.")
                    .foregroundStyle(.secondary)
            }
            ReminderSettings(model: model)
            Section {
                HStack {
                    Button("Export as JSON…") { model.exportJSON() }
                    Button("Save Backup…") { model.saveBackup() }
                    Button("Restore Backup…") { model.restoreBackup() }
                }
            } header: {
                Text("Data")
            } footer: {
                Text("JSON is readable anywhere. A backup holds everything with its history; restoring merges it in like a sync, so nothing here is lost.")
                    .foregroundStyle(.secondary)
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}

struct PairForm: View {
    let model: AppModel
    @State private var serverURL = ""
    @State private var code = ""
    @State private var deviceName = AppModel.defaultDeviceName
    @State private var pairing = false

    private var canPair: Bool {
        !pairing && !serverURL.trimmingCharacters(in: .whitespaces).isEmpty
            && !code.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        Form {
            Section {
                TextField("Server", text: $serverURL, prompt: Text("https://not.example.ts.net"))
                TextField("Pairing code", text: $code, prompt: Text("XXXXX-XXXXX"))
                    .font(.body.monospaced())
                TextField("Device name", text: $deviceName)
            } header: {
                Text("Pair this Mac")
            } footer: {
                Text("Get a code on the server with `not-server pair`, or on a paired device under Sync → Add Device.")
                    .foregroundStyle(.secondary)
            }
            HStack {
                Spacer()
                if pairing { ProgressView().controlSize(.small) }
                Button("Pair") {
                    pairing = true
                    Task {
                        await model.pair(serverURL: serverURL, code: code, deviceName: deviceName)
                        pairing = false
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(!canPair)
            }
        }
    }
}

private struct PairedView: View {
    let model: AppModel
    let info: SyncInfo
    @State private var devices: [Device] = []
    @State private var code: PairingCode?
    @State private var pendingRevoke: Device?
    @State private var confirmingUnpair = false

    /// The server no longer knows this device (revoked, or its database was reset), so only
    /// unpairing makes sense.
    private var rejected: Bool { model.syncState == .unauthorized }

    var body: some View {
        Form {
            Section("Sync") {
                LabeledContent("Server", value: info.serverUrl)
                LabeledContent("This Mac", value: info.deviceName)
                LabeledContent("Status", value: status)
                HStack {
                    Spacer()
                    // Lives here, not in the device list, which can't load once the server rejects us.
                    Button("Unpair…", role: .destructive) { confirmingUnpair = true }
                }
            }
            if !rejected { otherSections }
        }
        .frame(minHeight: rejected ? 0 : 640)
        .task(id: [info.deviceId, rejected ? 1 : 0]) { if !rejected { devices = await model.devices() } }
        .confirmationDialog("Unpair this Mac?", isPresented: $confirmingUnpair) {
            Button("Unpair", role: .destructive) { Task { await model.unpair() } }
        } message: {
            Text(rejected
                ? "Tasks stay on this Mac. Pair again to resume syncing."
                : "Sync stops and this Mac is removed from the server. Tasks stay on this Mac.")
        }
        .confirmationDialog(
            pendingRevoke.map { "Revoke \($0.name)?" } ?? "",
            isPresented: Binding(get: { pendingRevoke != nil }, set: { if !$0 { pendingRevoke = nil } }),
            presenting: pendingRevoke
        ) { device in
            Button("Revoke", role: .destructive) {
                Task {
                    await model.revoke(device)
                    devices = await model.devices()
                }
            }
        } message: { device in
            Text("\(device.name) stops syncing until it is paired again. Its local data stays on it.")
        }
    }

    @ViewBuilder private var otherSections: some View {
        Section {
            if let code {
                HStack {
                    Spacer()
                    QRCodeView(text: pairingLink(serverUrl: info.serverUrl, code: code.code))
                    Spacer()
                }
                LabeledContent("Pairing code") {
                    Text(code.code).font(.title2.monospaced()).textSelection(.enabled)
                }
                Text("Works once, until \(date(code.expiresAt).formatted(date: .omitted, time: .shortened)).")
                    .foregroundStyle(.secondary)
            } else {
                Button("Add Device…") { Task { code = await model.createPairingCode() } }
            }
        } header: {
            Text("Add a device")
        } footer: {
            Text("Scan the QR code with the Android app, or enter the code there along with this server's URL.")
                .foregroundStyle(.secondary)
        }
        Section("Devices") {
            ForEach(devices, id: \.id) { device in
                HStack {
                    VStack(alignment: .leading) {
                        Text(device.isCurrent ? "\(device.name) (this Mac)" : device.name)
                        Text(lastSeen(device)).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    if !device.isCurrent {
                        Button("Revoke…", role: .destructive) { pendingRevoke = device }
                    }
                }
            }
        }
    }

    private var status: String {
        switch model.syncState {
        case nil: "Starting…"
        case .syncing: "Syncing…"
        case .synced: "Up to date"
        case let .offline(reason): "Offline: \(reason)"
        case .unauthorized: "Rejected by the server. Unpair, then pair again."
        }
    }

    private func lastSeen(_ device: Device) -> String {
        guard let seen = device.lastSeenAt else { return "Never synced" }
        return "Last seen " + date(seen).formatted(.relative(presentation: .named))
    }
}

/// A QR code drawn with hard edges, on white so it scans in dark mode too.
private struct QRCodeView: View {
    let text: String

    var body: some View {
        if let image = Self.render(text) {
            Image(decorative: image, scale: 1)
                .interpolation(.none)
                .resizable()
                .frame(width: 180, height: 180)
                .padding(10)
                .background(.white, in: RoundedRectangle(cornerRadius: 8))
        }
    }

    private static func render(_ text: String) -> CGImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        return CIContext().createCGImage(output, from: output.extent)
    }
}

private func date(_ millis: Int64) -> Date {
    Date(timeIntervalSince1970: TimeInterval(millis) / 1000)
}
