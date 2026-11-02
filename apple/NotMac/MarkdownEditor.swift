import AppKit
import NotCore
import SwiftUI

/// A plain-text markdown editor that styles the markdown in place (headings, emphasis, checklists),
/// using the core's `markdownSpans` so it renders like the Android app. Click a checkbox to tick it;
/// Return continues lists; ⌘B / ⌘I wrap the selection.
struct MarkdownEditor: NSViewRepresentable {
    @Binding var text: String
    /// Bumped by the owner to move keyboard focus here.
    var focusRequests = 0

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = MarkdownTextView.scrollableTextView()
        scroll.drawsBackground = false
        let textView = scroll.documentView as! NSTextView
        // Swap in our subclass, keeping the configured text container.
        let editor = MarkdownTextView(frame: textView.frame, textContainer: textView.textContainer)
        editor.autoresizingMask = textView.autoresizingMask
        editor.isVerticallyResizable = true
        editor.minSize = textView.minSize
        editor.maxSize = textView.maxSize
        editor.delegate = context.coordinator
        editor.isRichText = false
        editor.allowsUndo = true
        editor.drawsBackground = false
        editor.isAutomaticQuoteSubstitutionEnabled = false
        editor.isAutomaticDashSubstitutionEnabled = false
        editor.textContainerInset = NSSize(width: 28, height: 22)
        editor.typingAttributes = MarkdownStyler.base
        editor.string = text
        MarkdownStyler.apply(to: editor)
        scroll.documentView = editor
        context.coordinator.textView = editor
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let editor = context.coordinator.textView else { return }
        if editor.string != text {
            let selection = editor.selectedRange()
            editor.string = text
            MarkdownStyler.apply(to: editor)
            let length = (text as NSString).length
            editor.setSelectedRange(NSRange(location: min(selection.location, length), length: 0))
        }
        if context.coordinator.focusRequests != focusRequests {
            context.coordinator.focusRequests = focusRequests
            DispatchQueue.main.async { editor.window?.makeFirstResponder(editor) }
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: MarkdownEditor
        weak var textView: MarkdownTextView?
        var focusRequests: Int

        init(_ parent: MarkdownEditor) {
            self.parent = parent
            focusRequests = parent.focusRequests
        }

        func textDidChange(_ notification: Notification) {
            guard let textView else { return }
            MarkdownStyler.apply(to: textView)
            parent.text = textView.string
        }

        func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
            return (textView as? MarkdownTextView)?.continueList() ?? false
        }
    }
}

final class MarkdownTextView: NSTextView {
    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let index = characterIndexForInsertion(at: point)
        if let box = checkbox(at: index) {
            let mark = (string as NSString).substring(with: NSRange(location: box.location + 1, length: 1))
            replace(NSRange(location: box.location + 1, length: 1), with: mark == " " ? "x" : " ")
            return
        }
        super.mouseDown(with: event)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard window?.firstResponder === self, event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command
        else { return super.performKeyEquivalent(with: event) }
        switch event.charactersIgnoringModifiers {
        case "b": wrapSelection("**")
        case "i": wrapSelection("*")
        default: return super.performKeyEquivalent(with: event)
        }
        return true
    }

    /// Return inside a list item starts the next item; on an empty item it ends the list.
    func continueList() -> Bool {
        let ns = string as NSString
        let caret = selectedRange().location
        let lineRange = ns.lineRange(for: NSRange(location: caret, length: 0))
        let line = ns.substring(with: NSRange(location: lineRange.location, length: caret - lineRange.location))
        guard let match = line.firstMatch(of: #/^(\s*)([-*+] \[[ xX]\] |[-*+] |(\d+)\. )/#) else { return false }
        let marker = String(match.output.0)
        if line == marker {
            // Empty item: drop the marker instead of adding another.
            replace(NSRange(location: lineRange.location, length: (marker as NSString).length), with: "")
            return true
        }
        var next = String(match.output.1) + String(match.output.2)
        if let number = match.output.3.flatMap({ Int($0) }) {
            next = String(match.output.1) + "\(number + 1). "
        } else if next.contains("[") {
            next = String(match.output.1) + String(match.output.2.prefix(1)) + " [ ] "
        }
        insertText("\n" + next, replacementRange: selectedRange())
        return true
    }

    private func wrapSelection(_ marker: String) {
        let range = selectedRange()
        let selected = (string as NSString).substring(with: range)
        replace(range, with: marker + selected + marker)
        setSelectedRange(NSRange(location: range.location + (marker as NSString).length, length: range.length))
    }

    /// An undoable edit that goes through the delegate like typing.
    private func replace(_ range: NSRange, with text: String) {
        guard shouldChangeText(in: range, replacementString: text) else { return }
        textStorage?.replaceCharacters(in: range, with: text)
        didChangeText()
    }

    private func checkbox(at index: Int) -> NSRange? {
        for span in markdownSpans(text: string) {
            guard case .checkbox = span.style else { continue }
            let range = NSRange(location: Int(span.start), length: Int(span.end - span.start))
            if NSLocationInRange(index, range) || index == NSMaxRange(range) - 1 { return range }
        }
        return nil
    }
}

/// Turns core markdown spans into text attributes.
@MainActor
enum MarkdownStyler {
    static let bodySize: CGFloat = 15
    static var bodyFont: NSFont { .systemFont(ofSize: bodySize) }
    static var base: [NSAttributedString.Key: Any] {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 3
        paragraph.paragraphSpacing = 4
        return [.font: bodyFont, .foregroundColor: NSColor.labelColor, .paragraphStyle: paragraph]
    }

    static func apply(to textView: NSTextView) {
        guard let storage = textView.textStorage else { return }
        let full = NSRange(location: 0, length: storage.length)
        let spans = markdownSpans(text: storage.string).map {
            (range: NSRange(location: Int($0.start), length: Int($0.end - $0.start)), style: $0.style)
        }
        storage.beginEditing()
        storage.setAttributes(base, range: full)
        // Fonts first (headings, code), then traits on top of them, then colors and decorations.
        for (range, style) in spans {
            switch style {
            case let .heading(level):
                let size: CGFloat = [26, 21, 17][Int(level) - 1]
                storage.addAttribute(.font, value: NSFont.systemFont(ofSize: size, weight: level == 3 ? .semibold : .bold), range: range)
            case .code:
                storage.addAttribute(.font, value: NSFont.monospacedSystemFont(ofSize: bodySize - 1, weight: .regular), range: range)
                storage.addAttribute(.backgroundColor, value: NSColor.quaternaryLabelColor.withAlphaComponent(0.12), range: range)
            default: break
            }
        }
        for (range, style) in spans {
            let trait: NSFontTraitMask? = switch style {
            case .bold: .boldFontMask
            case .italic, .quote: .italicFontMask
            default: nil
            }
            guard let trait else { continue }
            storage.enumerateAttribute(.font, in: range) { value, run, _ in
                guard let font = value as? NSFont else { return }
                storage.addAttribute(.font, value: NSFontManager.shared.convert(font, toHaveTrait: trait), range: run)
            }
        }
        for (range, style) in spans {
            switch style {
            case .syntax:
                storage.addAttribute(.foregroundColor, value: NSColor.tertiaryLabelColor, range: range)
            case .quote:
                storage.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: range)
            case .bullet:
                storage.addAttribute(.foregroundColor, value: NSColor.controlAccentColor, range: range)
            case .checkbox:
                storage.addAttribute(.foregroundColor, value: NSColor.controlAccentColor, range: range)
                storage.addAttribute(.font, value: NSFont.monospacedSystemFont(ofSize: bodySize, weight: .semibold), range: range)
                storage.addAttribute(.cursor, value: NSCursor.pointingHand, range: range)
            case .done, .strike:
                storage.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
                storage.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: range)
            default: break
            }
        }
        storage.endEditing()
        textView.typingAttributes = base
    }
}
