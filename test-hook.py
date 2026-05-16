#!/usr/bin/env python3
"""
Comprehensive test for AgentBro hook event pipeline.

Simulates a full Claude Code session lifecycle by sending events directly
to the HookServer Unix socket (or TCP fallback). This tests the path:

  test-hook.py -> Socket -> HookServer -> SessionStore -> Tauri emit -> Frontend

Usage:
    python3 test-hook.py              # Full session test
    python3 test-hook.py --quick      # Quick smoke test (3 events)
    python3 test-hook.py --permission # Test permission request flow
    python3 test-hook.py --question   # Test AskQuestion / AskUserQuestion UI flow
    python3 test-hook.py --plan       # Test PlanApproval / ExitPlanMode UI flow
    python3 test-hook.py --tcp        # Use TCP instead of Unix socket
"""
import json
import socket
import sys
import time
import argparse

UNIX_SOCKET_PATH = "/tmp/agentbro.sock"
TCP_HOST = "127.0.0.1"
TCP_PORT = 17892

SESSION_ID = f"test-{int(time.time())}"
CWD = "/tmp/test-project"
PID = 12345
TTY = "/dev/ttys099"


class Colors:
    GREEN = "\033[92m"
    RED = "\033[91m"
    YELLOW = "\033[93m"
    CYAN = "\033[96m"
    DIM = "\033[2m"
    RESET = "\033[0m"
    BOLD = "\033[1m"


def connect(use_tcp=False):
    """Connect to AgentBro server."""
    if not use_tcp:
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.settimeout(5)
            sock.connect(UNIX_SOCKET_PATH)
            return sock, "unix"
        except (socket.error, OSError):
            pass

    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5)
        sock.connect((TCP_HOST, TCP_PORT))
        return sock, "tcp"
    except (socket.error, OSError):
        return None, None


def send_event(event_data, use_tcp=False, wait_response=False):
    """Send a JSON event and optionally wait for response."""
    sock, transport = connect(use_tcp)
    if not sock:
        print(f"  {Colors.RED}FAILED: Cannot connect (tried {'TCP' if use_tcp else 'Unix+TCP'}){Colors.RESET}")
        return False, None

    try:
        payload = json.dumps(event_data) + "\n"
        sock.sendall(payload.encode())

        response = None
        if wait_response:
            try:
                data = b""
                while True:
                    chunk = sock.recv(4096)
                    if not chunk:
                        break
                    data += chunk
                    if b"\n" in data:
                        break
                if data:
                    response = json.loads(data.decode().strip())
            except socket.timeout:
                pass

        sock.close()
        event_name = event_data.get("event", "unknown")
        status = event_data.get("status", "")
        tool = event_data.get("tool", "")
        detail = f" [{status}]" if status else ""
        detail += f" tool={tool}" if tool else ""
        print(f"  {Colors.GREEN}OK{Colors.RESET} via {transport}{Colors.DIM}{detail}{Colors.RESET}")
        if response:
            print(f"  {Colors.CYAN}Response: {json.dumps(response)}{Colors.RESET}")
        return True, response
    except Exception as e:
        print(f"  {Colors.RED}FAILED: {e}{Colors.RESET}")
        try:
            sock.close()
        except Exception:
            pass
        return False, None


def test_full_session(use_tcp=False):
    """Simulate a complete Claude Code session lifecycle."""
    print(f"{Colors.BOLD}=== AgentBro Full Session Test ==={Colors.RESET}")
    print(f"Socket: {Colors.DIM}{UNIX_SOCKET_PATH}{Colors.RESET}")
    print(f"TCP:    {Colors.DIM}{TCP_HOST}:{TCP_PORT}{Colors.RESET}")
    print(f"Session: {Colors.CYAN}{SESSION_ID}{Colors.RESET}")
    print()

    passed = 0
    failed = 0
    total = 0

    def step(num, label, event_data, **kwargs):
        nonlocal passed, failed, total
        total += 1
        print(f"{Colors.BOLD}{num}. {label}{Colors.RESET}")
        ok, resp = send_event(event_data, use_tcp=use_tcp, **kwargs)
        if ok:
            passed += 1
        else:
            failed += 1
        return ok, resp

    # 1. SessionStart - new session begins
    step(1, "SessionStart", {
        "agent": "claude-code",
        "event": "SessionStart",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "waiting_for_input",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 2. UserPromptSubmit - user sends a message
    step(2, "UserPromptSubmit (user sends message)", {
        "agent": "claude-code",
        "event": "UserPromptSubmit",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "processing",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 3. PreToolUse - tool starts (Read)
    step(3, "PreToolUse (Read file)", {
        "agent": "claude-code",
        "event": "PreToolUse",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "running_tool",
        "tool": "Read",
        "tool_input": {"file_path": "/tmp/test.txt"},
        "tool_use_id": "tu_001",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 4. PostToolUse - tool completes (Read)
    step(4, "PostToolUse (Read complete)", {
        "agent": "claude-code",
        "event": "PostToolUse",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "processing",
        "tool": "Read",
        "tool_input": {"file_path": "/tmp/test.txt"},
        "tool_use_id": "tu_001",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 5. PreToolUse - tool starts (Bash)
    step(5, "PreToolUse (Bash command)", {
        "agent": "claude-code",
        "event": "PreToolUse",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "running_tool",
        "tool": "Bash",
        "tool_input": {"command": "ls -la /tmp"},
        "tool_use_id": "tu_002",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 6. PostToolUseFailure - tool fails
    step(6, "PostToolUseFailure (Bash errored)", {
        "agent": "claude-code",
        "event": "PostToolUseFailure",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "processing",
        "tool": "Bash",
        "tool_input": {"command": "ls -la /tmp"},
        "tool_use_id": "tu_002",
        "error": "Command timed out after 120s",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 7. PreToolUse - another tool (Edit)
    step(7, "PreToolUse (Edit file)", {
        "agent": "claude-code",
        "event": "PreToolUse",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "running_tool",
        "tool": "Edit",
        "tool_input": {"file_path": "/tmp/test.txt", "old_string": "foo", "new_string": "bar"},
        "tool_use_id": "tu_003",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 8. PostToolUse - Edit completes
    step(8, "PostToolUse (Edit complete)", {
        "agent": "claude-code",
        "event": "PostToolUse",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "processing",
        "tool": "Edit",
        "tool_input": {"file_path": "/tmp/test.txt", "old_string": "foo", "new_string": "bar"},
        "tool_use_id": "tu_003",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 9. Notification (status update)
    step(9, "Notification (status update)", {
        "agent": "claude-code",
        "event": "Notification",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "notification",
        "notification_type": "status",
        "message": "Analyzing codebase structure...",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 10. SubagentStart
    step(10, "SubagentStart (spawning subagent)", {
        "agent": "claude-code",
        "event": "SubagentStart",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "processing",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 11. SubagentStop
    step(11, "SubagentStop (subagent done)", {
        "agent": "claude-code",
        "event": "SubagentStop",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "processing",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 12. PreCompact
    step(12, "PreCompact (context compaction)", {
        "agent": "claude-code",
        "event": "PreCompact",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "compacting",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 13. PostCompact
    step(13, "PostCompact (compaction done)", {
        "agent": "claude-code",
        "event": "PostCompact",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "processing",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 14. PermissionDenied (auto-mode denied a tool)
    step(14, "PermissionDenied (auto-mode blocked)", {
        "agent": "claude-code",
        "event": "PermissionDenied",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "processing",
        "tool": "Bash",
        "tool_input": {"command": "rm -rf /"},
        "reason": "Command matches deny rule",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 15. Stop (turn ends, waiting for user)
    step(15, "Stop (waiting for input)", {
        "agent": "claude-code",
        "event": "Stop",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "waiting_for_input",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(1)

    # 16. UserPromptSubmit - user sends another message
    step(16, "UserPromptSubmit (second turn)", {
        "agent": "claude-code",
        "event": "UserPromptSubmit",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "processing",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(0.5)

    # 17. StopFailure (API error)
    step(17, "StopFailure (rate limited)", {
        "agent": "claude-code",
        "event": "StopFailure",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "waiting_for_input",
        "error": "Rate limit exceeded, retry in 30s",
        "pid": PID,
        "tty": TTY,
    })
    time.sleep(1)

    # 18. SessionEnd
    step(18, "SessionEnd", {
        "agent": "claude-code",
        "event": "SessionEnd",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "ended",
        "pid": PID,
        "tty": TTY,
    })

    # Summary
    print()
    print(f"{Colors.BOLD}=== Results ==={Colors.RESET}")
    print(f"  Passed: {Colors.GREEN}{passed}/{total}{Colors.RESET}")
    if failed:
        print(f"  Failed: {Colors.RED}{failed}/{total}{Colors.RESET}")
    print()

    if failed == 0:
        print(f"{Colors.GREEN}All events delivered successfully!{Colors.RESET}")
        print(f"Check the AgentBro notch panel for session state changes.")
    else:
        print(f"{Colors.RED}Some events failed. Is AgentBro running?{Colors.RESET}")
        print(f"The app must be running to accept socket connections.")

    return failed == 0


def test_quick(use_tcp=False):
    """Quick 3-event smoke test."""
    print(f"{Colors.BOLD}=== Quick Smoke Test ==={Colors.RESET}")
    print(f"Session: {Colors.CYAN}{SESSION_ID}{Colors.RESET}")
    print()

    ok1, _ = send_event({
        "agent": "claude-code", "event": "SessionStart",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "waiting_for_input", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)
    print(f"  1. SessionStart: {'OK' if ok1 else 'FAIL'}")

    time.sleep(0.3)

    ok2, _ = send_event({
        "agent": "claude-code", "event": "UserPromptSubmit",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "processing", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)
    print(f"  2. Processing:   {'OK' if ok2 else 'FAIL'}")

    time.sleep(0.3)

    ok3, _ = send_event({
        "agent": "claude-code", "event": "SessionEnd",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "ended", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)
    print(f"  3. SessionEnd:   {'OK' if ok3 else 'FAIL'}")

    print()
    all_ok = ok1 and ok2 and ok3
    if all_ok:
        print(f"{Colors.GREEN}Smoke test passed!{Colors.RESET}")
    else:
        print(f"{Colors.RED}Smoke test failed. Is AgentBro running?{Colors.RESET}")
    return all_ok


def test_permission(use_tcp=False):
    """Test the permission request flow (will block waiting for UI response)."""
    print(f"{Colors.BOLD}=== Permission Request Test ==={Colors.RESET}")
    print(f"Session: {Colors.CYAN}{SESSION_ID}{Colors.RESET}")
    print()

    # Start session
    print("1. Starting session...")
    send_event({
        "agent": "claude-code", "event": "SessionStart",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "waiting_for_input", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)
    time.sleep(0.3)

    # Send processing
    print("2. Processing...")
    send_event({
        "agent": "claude-code", "event": "UserPromptSubmit",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "processing", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)
    time.sleep(0.3)

    # Send permission request (this will BLOCK until UI responds)
    print("3. Sending PermissionRequest (waiting for UI decision)...")
    print(f"   {Colors.YELLOW}Approve or deny in the AgentBro UI...{Colors.RESET}")
    ok, response = send_event({
        "agent": "claude-code", "event": "PermissionRequest",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "waiting_for_approval",
        "tool": "Bash",
        "tool_input": {"command": "npm install express"},
        "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp, wait_response=True)

    if response:
        decision = response.get("decision", "unknown")
        reason = response.get("reason", "")
        print(f"\n   {Colors.GREEN}Decision received: {decision}{Colors.RESET}")
        if reason:
            print(f"   Reason: {reason}")
    else:
        print(f"\n   {Colors.YELLOW}No response (timed out or no decision){Colors.RESET}")

    # Cleanup
    time.sleep(0.3)
    print("4. Ending session...")
    send_event({
        "agent": "claude-code", "event": "SessionEnd",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "ended", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)

    print()
    print(f"{Colors.GREEN}Permission test complete!{Colors.RESET}")
    return True


def test_question(use_tcp=False):
    """Test the AskQuestion flow (will block waiting for UI response)."""
    print(f"{Colors.BOLD}=== AskQuestion Test ==={Colors.RESET}")
    print(f"Session: {Colors.CYAN}{SESSION_ID}{Colors.RESET}")
    print()

    print("1. Starting session...")
    send_event({
        "agent": "claude-code", "event": "SessionStart",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "waiting_for_input", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)
    time.sleep(0.3)

    print("2. Sending AskQuestion (waiting for UI answer)...")
    print(f"   {Colors.YELLOW}Answer in the AgentBro UI...{Colors.RESET}")
    ok, response = send_event({
        "agent": "claude-code",
        "event": "AskQuestion",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "waiting_for_input",
        "question": "[Deploy] Choose release options",
        "options": ["Preview", "Ship"],
        "descriptions": ["Open staging first", "Release now"],
        "header": "Deploy",
        "multiSelect": True,
        "questions": [
            {
                "header": "Deploy",
                "question": "Which target?",
                "options": [
                    {"label": "Preview", "description": "Open staging first"},
                    {"label": "Ship", "description": "Release now"},
                ],
                "multiSelect": True,
            },
            {
                "header": "Notify",
                "question": "Notify the team?",
                "options": [{"label": "Yes"}, {"label": "No"}],
                "multiSelect": False,
            },
        ],
        "pid": PID,
        "tty": TTY,
    }, use_tcp=use_tcp, wait_response=True)

    if response:
        print(f"\n   {Colors.GREEN}Answer received: {json.dumps(response)}{Colors.RESET}")
    elif ok:
        print(f"\n   {Colors.YELLOW}No response (timed out or no answer){Colors.RESET}")

    time.sleep(0.3)
    print("3. Ending session...")
    send_event({
        "agent": "claude-code", "event": "SessionEnd",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "ended", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)

    return ok


def test_plan(use_tcp=False):
    """Test the PlanApproval flow (will block waiting for UI response)."""
    print(f"{Colors.BOLD}=== PlanApproval Test ==={Colors.RESET}")
    print(f"Session: {Colors.CYAN}{SESSION_ID}{Colors.RESET}")
    print()

    print("1. Starting session...")
    send_event({
        "agent": "claude-code", "event": "SessionStart",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "waiting_for_input", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)
    time.sleep(0.3)

    print("2. Sending PlanApproval (waiting for UI decision)...")
    print(f"   {Colors.YELLOW}Choose Manual / Accept Edits / Auto in AgentBro...{Colors.RESET}")
    ok, response = send_event({
        "agent": "claude-code",
        "event": "PlanApproval",
        "session_id": SESSION_ID,
        "cwd": CWD,
        "status": "waiting_for_approval",
        "plan_title": "Implement checkout",
        "plan_content": "1. Update API\n2. Adjust UI\n3. Run tests",
        "requested_permissions": [
            {"tool": "Edit", "prompt": "Update checkout.ts"},
            "Bash: npm test",
        ],
        "pid": PID,
        "tty": TTY,
    }, use_tcp=use_tcp, wait_response=True)

    if response:
        print(f"\n   {Colors.GREEN}Plan response received: {json.dumps(response)}{Colors.RESET}")
    elif ok:
        print(f"\n   {Colors.YELLOW}No response (timed out or no decision){Colors.RESET}")

    time.sleep(0.3)
    print("3. Ending session...")
    send_event({
        "agent": "claude-code", "event": "SessionEnd",
        "session_id": SESSION_ID, "cwd": CWD,
        "status": "ended", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)

    return ok


def test_multi_session(use_tcp=False):
    """Test multiple concurrent sessions."""
    print(f"{Colors.BOLD}=== Multi-Session Test ==={Colors.RESET}")
    print()

    sessions = [
        (f"test-a-{int(time.time())}", "/Users/dev/project-alpha", "Project Alpha"),
        (f"test-b-{int(time.time())}", "/Users/dev/project-beta", "Project Beta"),
        (f"test-c-{int(time.time())}", "/Users/dev/project-gamma", "Project Gamma"),
    ]

    # Start all sessions
    for sid, cwd, label in sessions:
        print(f"Starting {label} ({sid[:12]}...)...")
        send_event({
            "agent": "claude-code", "event": "SessionStart",
            "session_id": sid, "cwd": cwd,
            "status": "waiting_for_input", "pid": PID, "tty": TTY,
        }, use_tcp=use_tcp)
        time.sleep(0.2)

    print()
    time.sleep(0.5)

    # Put them in different states
    print(f"Setting {sessions[0][2]} to processing...")
    send_event({
        "agent": "claude-code", "event": "UserPromptSubmit",
        "session_id": sessions[0][0], "cwd": sessions[0][1],
        "status": "processing", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)
    time.sleep(0.3)

    print(f"Setting {sessions[1][2]} to running tool...")
    send_event({
        "agent": "claude-code", "event": "PreToolUse",
        "session_id": sessions[1][0], "cwd": sessions[1][1],
        "status": "running_tool", "tool": "Grep",
        "tool_input": {"pattern": "TODO", "path": "."},
        "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)
    time.sleep(0.3)

    print(f"Setting {sessions[2][2]} to compacting...")
    send_event({
        "agent": "claude-code", "event": "PreCompact",
        "session_id": sessions[2][0], "cwd": sessions[2][1],
        "status": "compacting", "pid": PID, "tty": TTY,
    }, use_tcp=use_tcp)

    print()
    print(f"{Colors.YELLOW}Check UI -- you should see 3 sessions in different states.{Colors.RESET}")
    print("Cleaning up in 5 seconds...")
    time.sleep(5)

    # End all sessions
    for sid, cwd, label in sessions:
        send_event({
            "agent": "claude-code", "event": "SessionEnd",
            "session_id": sid, "cwd": cwd,
            "status": "ended", "pid": PID, "tty": TTY,
        }, use_tcp=use_tcp)
        time.sleep(0.1)

    print(f"{Colors.GREEN}Multi-session test complete!{Colors.RESET}")
    return True


def main():
    parser = argparse.ArgumentParser(description="AgentBro Hook Test")
    parser.add_argument("--quick", action="store_true", help="Quick 3-event smoke test")
    parser.add_argument("--permission", action="store_true", help="Test permission request flow")
    parser.add_argument("--question", action="store_true", help="Test AskQuestion flow")
    parser.add_argument("--plan", action="store_true", help="Test PlanApproval flow")
    parser.add_argument("--multi", action="store_true", help="Test multiple concurrent sessions")
    parser.add_argument("--tcp", action="store_true", help="Force TCP connection")
    args = parser.parse_args()

    # Check connectivity first
    sock, transport = connect(args.tcp)
    if not sock:
        print(f"{Colors.RED}Cannot connect to AgentBro.{Colors.RESET}")
        print(f"Make sure the app is running and listening on:")
        print(f"  Unix: {UNIX_SOCKET_PATH}")
        print(f"  TCP:  {TCP_HOST}:{TCP_PORT}")
        sys.exit(1)
    sock.close()
    print(f"{Colors.GREEN}Connected to AgentBro via {transport}{Colors.RESET}")
    print()

    if args.quick:
        success = test_quick(args.tcp)
    elif args.permission:
        success = test_permission(args.tcp)
    elif args.question:
        success = test_question(args.tcp)
    elif args.plan:
        success = test_plan(args.tcp)
    elif args.multi:
        success = test_multi_session(args.tcp)
    else:
        success = test_full_session(args.tcp)

    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
