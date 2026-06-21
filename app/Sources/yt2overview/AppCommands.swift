import AppKit
import SwiftUI

/// Full menu bar for yt2overview: app/info, settings, generate/clear, copy, view modes, history.
struct AppCommands: Commands {
    @Bindable var model: AppModel

    private var urlEmpty: Bool {
        model.url.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some Commands {
        // App menu → About + Settings
        CommandGroup(replacing: .appInfo) {
            Button("About yt2overview") { showAboutPanel() }
        }
        CommandGroup(replacing: .appSettings) {
            Button("Settings…") { model.showSettings = true }
                .keyboardShortcut(",", modifiers: .command)
        }

        // File menu → Generate / Paste & Generate / Clear
        CommandGroup(replacing: .newItem) {
            Button("Generate Overview") { model.generate() }
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(urlEmpty || model.isBusy)
            Button("Paste Link & Generate") {
                model.pasteURL()
                model.generate()
            }
            .keyboardShortcut("v", modifiers: [.command, .shift])
            Divider()
            Button("Clear") { model.clear() }
                .keyboardShortcut(.delete, modifiers: .command)
                .disabled(!model.hasResult && urlEmpty)
        }

        // Dedicated Overview menu
        CommandMenu("Overview") {
            Button("Copy All") { model.copyCurrent() }
                .keyboardShortcut("c", modifiers: [.command, .shift])
                .disabled(!model.hasResult)
            Button("Copy AI-Optimized Payload") { model.copyAI() }
                .keyboardShortcut("c", modifiers: [.command, .option])
                .disabled(!model.hasResult)
            Divider()
            Button("Show Readable") { model.outputMode = "human" }
                .keyboardShortcut("1", modifiers: .command)
                .disabled(!model.hasResult)
            Button("Show AI-Optimized") { model.outputMode = "ai" }
                .keyboardShortcut("2", modifiers: .command)
                .disabled(!model.hasResult)
        }

        // History menu
        CommandMenu("History") {
            Button("Show History…") { model.showHistory = true }
                .keyboardShortcut("y", modifiers: .command)
            Divider()
            if model.history.entries.isEmpty {
                Text("No history yet")
            } else {
                ForEach(model.history.entries.prefix(10)) { entry in
                    Button(entry.title) { model.load(entry) }
                }
                Divider()
                Button("Clear History") { model.history.clear() }
            }
        }
    }

    private func showAboutPanel() {
        NSApp.activate(ignoringOtherApps: true)
        NSApp.orderFrontStandardAboutPanel(options: [
            .applicationName: "yt2overview",
            .applicationVersion: "1.0",
            .credits: NSAttributedString(
                string: "Local media → AI-ready overview.\nTranscript · comments · visual + AI summary, generated on-device.",
                attributes: [.font: NSFont.systemFont(ofSize: 11)]
            ),
        ])
    }
}
