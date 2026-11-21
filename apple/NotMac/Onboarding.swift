import AppKit
import SwiftUI
import UserNotifications

/// Whether the first-run steps are behind us. A Mac that's paired or already holds data counts as
/// done. Mirrors `FirstRun` on Android.
enum FirstRun {
    static let key = "onboarded"

    @MainActor
    static func needed(_ model: AppModel) -> Bool {
        #if DEBUG
        // Dev runs: `-firstRun YES` shows it whatever the data.
        if UserDefaults.standard.bool(forKey: "firstRun") { return true }
        #endif
        if UserDefaults.standard.bool(forKey: key) { return false }
        if model.isEstablished {
            done()
            return false
        }
        return true
    }

    static func done() { UserDefaults.standard.set(true, forKey: key) }
}

/// Four short steps on a fresh Mac: what not is, where the data comes from (start fresh, with a few
/// tasks that show the basics, or restore a backup, or pair with a sync server), reminders and the
/// calendar, and where not lives around the Mac. Every step can be skipped; everything is in
/// Settings later.
struct OnboardingSheet: View {
    let model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var step = 0
    @State private var pairing = false
    @State private var examples = true
    @AppStorage(Reminders.tasksKey) private var taskReminders = true
    @AppStorage(MenuBarPanel.visibleKey) private var menuBar = true

    private let count = 4

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Group {
                switch step {
                case 0: welcome
                case 1: data
                case 2: reminders
                default: aroundTheMac
                }
            }
            .frame(maxWidth: .infinity, minHeight: 300, alignment: .topLeading)
            .transition(.push(from: .trailing))
            .id(step)

            HStack {
                HStack(spacing: 6) {
                    ForEach(0..<count, id: \.self) { i in
                        Capsule().fill(i == step ? AnyShapeStyle(.tint) : AnyShapeStyle(.quaternary))
                            .frame(width: i == step ? 14 : 6, height: 6)
                    }
                }
                Spacer()
                if step > 0 { Button("Back") { go(step - 1) } }
                if step < count - 1 {
                    Button(step == 0 ? "Get Started" : step == 1 && !model.isEstablished ? "Start Fresh" : "Continue") {
                        if step == 1 { startFresh() } else { go(step + 1) }
                    }
                        .keyboardShortcut(.defaultAction)
                } else {
                    Button("Start Using not") { finish() }
                        .keyboardShortcut(.defaultAction)
                }
            }
            .padding(.top, 16)
        }
        .padding(24)
        .frame(width: 520)
        .animation(.snappy, value: step)
        .onChange(of: model.syncInfo != nil) { _, paired in
            // Paired: the data is on its way, move on.
            if paired, step == 1 { go(2) }
        }
        .interactiveDismissDisabled()
    }

    private func go(_ to: Int) { step = to }

    /// On from the data step: with the example tasks, unless unticked or data came in already.
    private func startFresh() {
        if examples, !model.isEstablished { model.addStarterTasks() }
        go(2)
    }

    private func finish() {
        FirstRun.done()
        dismiss()
    }

    private var welcome: some View {
        VStack(alignment: .leading, spacing: 14) {
            Image(nsImage: NSApp.applicationIconImage).resizable().frame(width: 72, height: 72)
            Text("Welcome to not").font(.largeTitle.weight(.bold))
            Text("Tasks, notes, habits and workouts in one calm place. Everything stays on this Mac and works offline; syncing with your other devices is up to you.")
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 8) {
                tip("text.cursor", "Type tasks the way you'd say them: “call mum fri 6pm #home !1”.")
                tip("star", "Today gathers what's due, your habits and calendar.")
                tip("arrow.uturn.backward", "Nothing asks “are you sure?”: ⌘Z takes anything back.")
            }
            .padding(.top, 4)
            HStack {
                Spacer()
                Button("Skip Setup") { finish() }.buttonStyle(.link)
            }
        }
    }

    private var data: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Your data").font(.title.weight(.bold))
            Text("Start fresh, or bring what you have: a backup file, or your other devices through a sync server.")
                .foregroundStyle(.secondary)
            if pairing {
                PairForm(model: model)
                    .formStyle(.grouped)
                    .frame(height: 230)
            } else {
                choice("sparkles", "Start fresh", "Nothing to bring yet.") { startFresh() }
                Toggle("Add a few example tasks that show the basics", isOn: $examples)
                    .padding(.leading, 12)
                choice("doc.badge.arrow.up", "Restore a backup file", "A .notbackup saved from another device.") {
                    model.restoreBackup()
                    if model.isEstablished { go(2) }
                }
                choice("arrow.triangle.2.circlepath.icloud", "I have a sync server", "Get a code on a paired device: Settings → Sync.") { pairing = true }
            }
        }
    }

    private var reminders: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Reminders and calendar").font(.title.weight(.bold))
            Text("Both stay on this Mac; your phone has its own.").foregroundStyle(.secondary)
            Form {
                Toggle("Remind me when a task with a time is due", isOn: Binding(get: { taskReminders }, set: { on in
                    taskReminders = on
                    if on { Task { _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) } }
                    model.reminders.reschedule()
                }))
                Toggle("Show calendar events in Today and Upcoming", isOn: Binding(get: { model.calendar.enabled }, set: { model.calendar.enabled = $0 }))
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            .frame(height: 110)
            Text("Morning summaries, habit and timed habit reminders are in Settings → General.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    private var aroundTheMac: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Around your Mac").font(.title.weight(.bold))
            Text("not is a keystroke or a click away, whatever app is in front.").foregroundStyle(.secondary)
            Form {
                Toggle("Keep not in the menu bar", isOn: $menuBar)
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            .frame(height: 64)
            VStack(alignment: .leading, spacing: 8) {
                tip("checkmark.square", "The menu bar: add a task or tick one off without switching apps.")
                tip("square.and.arrow.down", "Services → Add to not: send selected text, a link or a picture from any app.")
                tip("keyboard", "⌘1–4 jump to Inbox, Today, Upcoming, Someday; ⌥⌘↑↓ step through lists.")
                tip("link", "Scripts, Shortcuts and Raycast can add tasks with not://add?title=…")
            }
        }
    }

    private func tip(_ symbol: String, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: symbol).foregroundStyle(.tint).frame(width: 20)
            Text(text)
        }
    }

    private func choice(_ symbol: String, _ title: String, _ detail: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Image(systemName: symbol).font(.title2).foregroundStyle(.tint).frame(width: 32)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.headline)
                    Text(detail).font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Image(systemName: "chevron.right").foregroundStyle(.tertiary)
            }
            .padding(12)
            .background(.quaternary.opacity(0.5), in: .rect(cornerRadius: 12))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}
