import CoreGraphics
import Foundation

struct DisplaySmokeError: LocalizedError {
    let errorDescription: String?

    init(_ message: String) {
        errorDescription = message
    }
}

func applyMode(_ mode: CGDisplayMode, to display: CGDirectDisplayID) throws {
    var pending: CGDisplayConfigRef?
    guard CGBeginDisplayConfiguration(&pending) == .success, let pending else {
        throw DisplaySmokeError("Could not start a display configuration")
    }
    let configured = CGConfigureDisplayWithDisplayMode(pending, display, mode, nil)
    guard configured == .success else {
        CGCancelDisplayConfiguration(pending)
        throw DisplaySmokeError("Could not configure the test display mode: \(configured.rawValue)")
    }
    let applied = CGCompleteDisplayConfiguration(pending, .forSession)
    guard applied == .success else {
        throw DisplaySmokeError("Could not apply the test display mode: \(applied.rawValue)")
    }
}

func islandBounds(processID: Int32) -> CGRect? {
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
        as? [[String: Any]] ?? []
    for window in windows {
        guard window[kCGWindowOwnerPID as String] as? Int32 == processID,
              let layer = window[kCGWindowLayer as String] as? Int,
              layer > Int(CGWindowLevelForKey(.screenSaverWindow)),
              let rawBounds = window[kCGWindowBounds as String] as? [String: Any],
              let bounds = CGRect(dictionaryRepresentation: rawBounds as CFDictionary) else { continue }
        return bounds
    }
    return nil
}

@discardableResult
func waitForIsland(_ display: CGDirectDisplayID, width: Int, processID: Int32, offset: CGFloat) throws -> CGRect {
    let deadline = ProcessInfo.processInfo.systemUptime + 20
    repeat {
        let screen = CGDisplayBounds(display)
        if Int(screen.width) == width, let bounds = islandBounds(processID: processID),
           abs(bounds.midX - screen.midX - offset) <= 2, abs(bounds.minY - screen.minY) <= 2 {
            print("Native Island center x=\(bounds.midX), screen center x=\(screen.midX), width=\(width).")
            return bounds
        }
        Thread.sleep(forTimeInterval: 0.1)
    } while ProcessInfo.processInfo.systemUptime < deadline
    throw DisplaySmokeError("Island did not reach offset \(offset) at width \(width). Bounds: \(String(describing: islandBounds(processID: processID))). Display: \(CGDisplayBounds(display)).")
}

func verifyDisplayChange() throws {
    guard ProcessInfo.processInfo.environment["RUNNER_ENVIRONMENT"] == "github-hosted" else {
        throw DisplaySmokeError("The display-mode check requires an isolated GitHub-hosted runner")
    }
    guard CommandLine.arguments.count == 3, let processID = Int32(CommandLine.arguments[1]),
          let offset = Double(CommandLine.arguments[2]) else {
        throw DisplaySmokeError("Expected the AgentBro PID and configured horizontal offset")
    }
    let display = CGMainDisplayID()
    guard let original = CGDisplayCopyDisplayMode(display) else {
        throw DisplaySmokeError("The hosted runner has no active display mode")
    }
    let options = [kCGDisplayShowDuplicateLowResolutionModes as String: true] as CFDictionary
    let modes = CGDisplayCopyAllDisplayModes(display, options) as? [CGDisplayMode] ?? []
    let sizes = Set(modes.map { "\($0.width)x\($0.height)" }).sorted()
    print("Hosted display modes: \(sizes.joined(separator: ", ")).")
    let initial = try waitForIsland(display, width: original.width, processID: processID, offset: offset)
    let minimumWidth = Int(ceil(initial.width + 2 * (abs(offset) + 8)))
    guard let alternate = modes.first(where: {
        $0.isUsableForDesktopGUI() && $0.width >= minimumWidth && $0.height >= 600
            && abs($0.width - original.width) >= 128
    }) else {
        throw DisplaySmokeError("No suitable alternate mode for \(original.width)x\(original.height)")
    }

    print("Changing the native macOS display from \(original.width)x\(original.height) to \(alternate.width)x\(alternate.height).")
    do {
        try applyMode(alternate, to: display)
        try waitForIsland(display, width: alternate.width, processID: processID, offset: offset)
    } catch {
        try applyMode(original, to: display)
        throw error
    }
    try applyMode(original, to: display)
    try waitForIsland(display, width: original.width, processID: processID, offset: offset)
}

do {
    try verifyDisplayChange()
} catch {
    FileHandle.standardError.write(Data("\(error.localizedDescription)\n".utf8))
    exit(1)
}
