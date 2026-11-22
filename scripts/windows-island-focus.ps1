Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public sealed class AgentBroIslandFocusTrace {
    private const uint ForegroundEvent = 0x0003;
    private const uint FocusEvent = 0x8005;
    private const uint SkipOwnProcess = 0x0002;
    private readonly IntPtr island;
    private readonly uint appProcess;
    private readonly List<string> events = new List<string>();
    private readonly ManualResetEventSlim ready = new ManualResetEventSlim();
    private readonly ManualResetEventSlim stop = new ManualResetEventSlim();
    private readonly Thread worker;
    private string error;

    [StructLayout(LayoutKind.Sequential)]
    private struct Message {
        public IntPtr Window;
        public uint Id;
        public UIntPtr WParam;
        public IntPtr LParam;
        public uint Time;
        public int X, Y;
        public uint Private;
    }

    private delegate void WinEvent(IntPtr hook, uint type, IntPtr window,
        int objectId, int childId, uint eventThread, uint eventTime);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetWinEventHook(uint first, uint last, IntPtr module,
        WinEvent callback, uint process, uint thread, uint flags);

    [DllImport("user32.dll")]
    private static extern bool UnhookWinEvent(IntPtr hook);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool PeekMessage(out Message message, IntPtr window, uint first, uint last, uint remove);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr DispatchMessage(ref Message message);

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern IntPtr GetAncestor(IntPtr window, uint flags);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassName(IntPtr window, StringBuilder name, int limit);

    [DllImport("kernel32.dll")]
    private static extern uint GetTickCount();

    public AgentBroIslandFocusTrace(IntPtr island, uint appProcess) {
        this.island = island;
        this.appProcess = appProcess;
        worker = new Thread(Capture) { IsBackground = true };
        worker.Start();
        if (!ready.Wait(2000) || error != null) {
            stop.Set();
            throw new InvalidOperationException(error ?? "The native focus trace did not start.");
        }
    }

    private void Capture() {
        WinEvent callback = Observe;
        var foreground = SetWinEventHook(ForegroundEvent, ForegroundEvent, IntPtr.Zero, callback, 0, 0, SkipOwnProcess);
        var focus = SetWinEventHook(FocusEvent, FocusEvent, IntPtr.Zero, callback, 0, 0, SkipOwnProcess);
        if (foreground == IntPtr.Zero || focus == IntPtr.Zero) {
            error = "Could not register the native focus event hooks.";
        }
        try {
            Observe(IntPtr.Zero, ForegroundEvent, GetForegroundWindow(), 0, 0, 0, GetTickCount());
            ready.Set();
            while (error == null && !stop.Wait(5)) {
                Message message;
                while (PeekMessage(out message, IntPtr.Zero, 0, 0, 1)) DispatchMessage(ref message);
            }
        } finally {
            if (foreground != IntPtr.Zero) UnhookWinEvent(foreground);
            if (focus != IntPtr.Zero) UnhookWinEvent(focus);
            GC.KeepAlive(callback);
        }
    }

    private void Observe(IntPtr hook, uint type, IntPtr window, int objectId,
        int childId, uint eventThread, uint eventTime) {
        uint process;
        GetWindowThreadProcessId(window, out process);
        if (type == FocusEvent && process != appProcess && GetAncestor(window, 2) != island) return;
        var name = new StringBuilder(128);
        GetClassName(window, name, name.Capacity);
        uint age = unchecked(GetTickCount() - eventTime);
        var at = DateTimeOffset.UtcNow.AddMilliseconds(-age);
        var kind = type == ForegroundEvent ? "foreground" : "focus";
        events.Add(string.Format("{0:O} {1} hwnd={2} class={3} process={4} thread={5} object={6} child={7}",
            at, kind, window, name, process, eventThread, objectId, childId));
        if (events.Count > 512) events.RemoveAt(0);
    }

    public string[] Finish() {
        stop.Set();
        if (!worker.Join(1500)) throw new InvalidOperationException("The native focus trace did not stop.");
        ready.Dispose();
        stop.Dispose();
        return events.ToArray();
    }
}
'@
