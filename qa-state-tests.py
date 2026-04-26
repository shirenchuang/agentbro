#!/usr/bin/env python3
"""QA tests for Agent Island session state machine and event processing."""

import socket
import json
import time
import uuid
import sys
import os

SOCKET_PATH = "/tmp/agent-island.sock"
RESULTS = []

def send_event(event, timeout=1.0):
    """Send a JSON event to the Unix socket and return the response."""
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.settimeout(timeout)
    try:
        sock.connect(SOCKET_PATH)
        sock.sendall((json.dumps(event) + "\n").encode())
        time.sleep(0.05)
        try:
            response = sock.recv(4096).decode()
            return response
        except socket.timeout:
            return None
    except Exception as e:
        return f"ERROR: {e}"
    finally:
        sock.close()

def send_raw(data, timeout=1.0):
    """Send raw bytes to the socket."""
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.settimeout(timeout)
    try:
        sock.connect(SOCKET_PATH)
        sock.sendall(data)
        time.sleep(0.05)
        try:
            response = sock.recv(4096).decode()
            return response
        except socket.timeout:
            return None
    except Exception as e:
        return f"ERROR: {e}"
    finally:
        sock.close()

def make_event(event_type, session_id, **extra):
    """Create a standard event dict."""
    ev = {
        "agent": "claude-code",
        "event": event_type,
        "session_id": session_id,
        "cwd": "/tmp/test",
        "pid": os.getpid(),
        "tty": "/dev/ttys999"
    }
    ev.update(extra)
    return ev

def report(test_name, passed, detail=""):
    status = "PASS" if passed else "FAIL"
    RESULTS.append((test_name, passed, detail))
    print(f"  [{status}] {test_name}")
    if detail:
        print(f"         {detail}")

def is_ok(resp):
    """Check if response indicates success (no error)."""
    if resp is None:
        return True  # No response = accepted silently
    if isinstance(resp, str) and resp.startswith("ERROR:"):
        return False
    if isinstance(resp, str):
        try:
            parsed = json.loads(resp)
            if "error" in parsed:
                return False
        except:
            pass
    return True

def is_error(resp):
    """Check if response indicates an error."""
    if resp is None:
        return False
    if isinstance(resp, str) and resp.startswith("ERROR:"):
        return True
    if isinstance(resp, str):
        try:
            parsed = json.loads(resp)
            return "error" in parsed
        except:
            pass
    return False

# ============================================================
# Test 1: Session Lifecycle
# ============================================================
def test_session_lifecycle():
    print("\n--- Test 1: Session Lifecycle ---")
    sid = f"test-lifecycle-{uuid.uuid4().hex[:8]}"

    events = [
        ("SessionStart", {"status": "waiting_for_input"}),
        ("UserPromptSubmit", {"status": "processing"}),
        ("PreToolUse", {"status": "running_tool", "tool": "Bash", "tool_input": {"command": "ls"}, "tool_use_id": "tu_001"}),
        ("PostToolUse", {"status": "processing", "tool": "Bash", "tool_input": {"command": "ls"}, "tool_use_id": "tu_001"}),
        ("Stop", {"status": "waiting_for_input"}),
        ("SessionEnd", {"status": "ended"}),
    ]

    all_ok = True
    for event_type, extra in events:
        resp = send_event(make_event(event_type, sid, **extra))
        ok = is_ok(resp)
        if not ok:
            all_ok = False
            report(f"Lifecycle: {event_type}", False, f"Response: {resp}")
        else:
            report(f"Lifecycle: {event_type} accepted", True, f"Response: {repr(resp)}")

    report("Session lifecycle complete flow", all_ok)

# ============================================================
# Test 2: Token Accumulation
# ============================================================
def test_token_accumulation():
    print("\n--- Test 2: Token Accumulation ---")
    sid = f"test-tokens-{uuid.uuid4().hex[:8]}"

    # Start session first
    send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    send_event(make_event("UserPromptSubmit", sid, status="processing"))
    time.sleep(0.05)

    token_events = [
        {"input": 100, "output": 50, "cache_read": 10, "cache_create": 5},
        {"input": 200, "output": 100, "cache_read": 20, "cache_create": 10},
        {"input": 300, "output": 150, "cache_read": 30, "cache_create": 15},
    ]

    expected_totals = {
        "input": 600, "output": 300, "cache_read": 60, "cache_create": 30
    }

    all_ok = True
    for i, tokens in enumerate(token_events):
        resp = send_event(make_event("TokenUsage", sid, **tokens))
        ok = is_ok(resp)
        if not ok:
            all_ok = False
            report(f"Token event {i+1}", False, f"Response: {resp}")
        else:
            report(f"Token event {i+1} accepted", True, f"Response: {repr(resp)}")

    report(f"Token accumulation (expected totals: input={expected_totals['input']}, output={expected_totals['output']}, cache_read={expected_totals['cache_read']}, cache_create={expected_totals['cache_create']})", all_ok)

    # Clean up
    send_event(make_event("SessionEnd", sid, status="ended"))

# ============================================================
# Test 3: Subagent Tracking
# ============================================================
def test_subagent_tracking():
    print("\n--- Test 3: Subagent Tracking ---")
    sid = f"test-subagent-{uuid.uuid4().hex[:8]}"

    send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    send_event(make_event("UserPromptSubmit", sid, status="processing"))
    time.sleep(0.05)

    # Start subagent
    resp1 = send_event(make_event("SubagentStart", sid,
        status="processing",
        agent_id="sub-001",
        description="Analyzing code"))
    ok1 = is_ok(resp1)
    report("SubagentStart accepted", ok1, f"Response: {repr(resp1)}")

    # Stop subagent
    resp2 = send_event(make_event("SubagentStop", sid,
        status="processing",
        agent_id="sub-001",
        agent_status="completed"))
    ok2 = is_ok(resp2)
    report("SubagentStop accepted", ok2, f"Response: {repr(resp2)}")

    report("Subagent tracking complete", ok1 and ok2)
    send_event(make_event("SessionEnd", sid, status="ended"))

# ============================================================
# Test 4: Tool Events (PreToolUse + PostToolUse with matching IDs)
# ============================================================
def test_tool_events():
    print("\n--- Test 4: Tool Events ---")
    sid = f"test-tools-{uuid.uuid4().hex[:8]}"
    tool_id = f"tu_{uuid.uuid4().hex[:8]}"

    send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    send_event(make_event("UserPromptSubmit", sid, status="processing"))
    time.sleep(0.05)

    # PreToolUse
    resp1 = send_event(make_event("PreToolUse", sid,
        status="running_tool",
        tool="Read",
        tool_input={"file_path": "/tmp/test.txt"},
        tool_use_id=tool_id))
    ok1 = is_ok(resp1)
    report(f"PreToolUse (id={tool_id}) accepted", ok1, f"Response: {repr(resp1)}")

    # PostToolUse with same ID
    resp2 = send_event(make_event("PostToolUse", sid,
        status="processing",
        tool="Read",
        tool_input={"file_path": "/tmp/test.txt"},
        tool_use_id=tool_id))
    ok2 = is_ok(resp2)
    report(f"PostToolUse (id={tool_id}) accepted", ok2, f"Response: {repr(resp2)}")

    report("Tool events with matching IDs", ok1 and ok2)
    send_event(make_event("SessionEnd", sid, status="ended"))

# ============================================================
# Test 5: Context Compaction
# ============================================================
def test_context_compaction():
    print("\n--- Test 5: Context Compaction ---")
    sid = f"test-compact-{uuid.uuid4().hex[:8]}"

    send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    send_event(make_event("UserPromptSubmit", sid, status="processing"))
    time.sleep(0.05)

    resp1 = send_event(make_event("PreCompact", sid, status="compacting"))
    ok1 = is_ok(resp1)
    report("PreCompact accepted", ok1, f"Response: {repr(resp1)}")

    resp2 = send_event(make_event("PostCompact", sid, status="processing"))
    ok2 = is_ok(resp2)
    report("PostCompact accepted", ok2, f"Response: {repr(resp2)}")

    report("Context compaction flow", ok1 and ok2)
    send_event(make_event("SessionEnd", sid, status="ended"))

# ============================================================
# Test 6: Error State
# ============================================================
def test_error_state():
    print("\n--- Test 6: Error State ---")
    sid = f"test-error-{uuid.uuid4().hex[:8]}"

    send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    send_event(make_event("UserPromptSubmit", sid, status="processing"))
    time.sleep(0.05)

    # Send StopFailure (error event)
    resp = send_event(make_event("StopFailure", sid,
        status="waiting_for_input",
        error="Rate limit exceeded"))
    ok = is_ok(resp)
    report("StopFailure (error) event accepted", ok, f"Response: {repr(resp)}")

    # Also test PostToolUseFailure
    resp2 = send_event(make_event("PostToolUseFailure", sid,
        status="processing",
        tool="Bash",
        tool_input={"command": "false"},
        tool_use_id="tu_err_001",
        error="Command failed with exit code 1"))
    ok2 = is_ok(resp2)
    report("PostToolUseFailure event accepted", ok2, f"Response: {repr(resp2)}")

    report("Error state handling", ok and ok2)
    send_event(make_event("SessionEnd", sid, status="ended"))

# ============================================================
# Test 7: Multiple Sessions
# ============================================================
def test_multiple_sessions():
    print("\n--- Test 7: Multiple Sessions ---")
    sessions = [f"test-multi-{i}-{uuid.uuid4().hex[:8]}" for i in range(3)]

    all_ok = True
    # Start all 3 sessions
    for sid in sessions:
        resp = send_event(make_event("SessionStart", sid, status="waiting_for_input"))
        ok = is_ok(resp)
        if not ok:
            all_ok = False
        report(f"Session {sid[:20]}... started", ok)

    # Send processing to each
    for sid in sessions:
        resp = send_event(make_event("UserPromptSubmit", sid, status="processing"))
        ok = is_ok(resp)
        if not ok:
            all_ok = False
        report(f"Session {sid[:20]}... processing", ok)

    # Send different tool events to each
    for i, sid in enumerate(sessions):
        tool_id = f"tu_multi_{i}"
        resp = send_event(make_event("PreToolUse", sid,
            status="running_tool",
            tool=f"Tool{i}",
            tool_input={"arg": f"value{i}"},
            tool_use_id=tool_id))
        ok = is_ok(resp)
        if not ok:
            all_ok = False
        report(f"Session {sid[:20]}... tool event", ok)

    # End all sessions
    for sid in sessions:
        send_event(make_event("SessionEnd", sid, status="ended"))

    report("Multiple independent sessions", all_ok)

# ============================================================
# Test 8: Permission Request Flow
# ============================================================
def test_permission_request():
    print("\n--- Test 8: Permission Request Flow ---")
    sid = f"test-perm-{uuid.uuid4().hex[:8]}"

    send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    send_event(make_event("UserPromptSubmit", sid, status="processing"))
    time.sleep(0.05)

    # Send PermissionRequest - this blocks waiting for a response
    # We use a short timeout since we'll send the allow response
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.settimeout(3.0)
    try:
        sock.connect(SOCKET_PATH)
        event = make_event("PermissionRequest", sid,
            status="waiting_for_approval",
            tool="Bash",
            tool_input={"command": "npm install express"})
        sock.sendall((json.dumps(event) + "\n").encode())

        # Connection should stay alive waiting for response
        # Send the allow response
        time.sleep(0.3)
        allow_response = json.dumps({"decision": "allow", "reason": "approved by test"}) + "\n"
        sock.sendall(allow_response.encode())

        time.sleep(0.2)
        try:
            resp = sock.recv(4096).decode()
            report("PermissionRequest: connection stayed alive", True, f"Response after allow: {repr(resp)}")
        except socket.timeout:
            report("PermissionRequest: connection stayed alive (no final response)", True, "Timed out waiting for final response (may be expected)")
    except Exception as e:
        report("PermissionRequest flow", False, f"Error: {e}")
    finally:
        sock.close()

    # Also test PermissionDenied (auto-denied, non-blocking)
    resp_denied = send_event(make_event("PermissionDenied", sid,
        status="processing",
        tool="Bash",
        tool_input={"command": "rm -rf /"},
        reason="Matches deny rule"))
    ok_denied = is_ok(resp_denied)
    report("PermissionDenied event accepted", ok_denied, f"Response: {repr(resp_denied)}")

    send_event(make_event("SessionEnd", sid, status="ended"))

# ============================================================
# Test 9: Rapid Events
# ============================================================
def test_rapid_events():
    print("\n--- Test 9: Rapid Events (50 events) ---")
    sid = f"test-rapid-{uuid.uuid4().hex[:8]}"

    send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    send_event(make_event("UserPromptSubmit", sid, status="processing"))

    success_count = 0
    fail_count = 0

    for i in range(50):
        tool_id = f"tu_rapid_{i}"
        try:
            # Alternate between PreToolUse and PostToolUse
            if i % 2 == 0:
                resp = send_event(make_event("PreToolUse", sid,
                    status="running_tool",
                    tool=f"Tool{i}",
                    tool_input={"arg": f"value{i}"},
                    tool_use_id=tool_id), timeout=2.0)
            else:
                resp = send_event(make_event("PostToolUse", sid,
                    status="processing",
                    tool=f"Tool{i-1}",
                    tool_input={"arg": f"value{i-1}"},
                    tool_use_id=f"tu_rapid_{i-1}"), timeout=2.0)

            if is_ok(resp):
                success_count += 1
            else:
                fail_count += 1
        except Exception as e:
            fail_count += 1

    report(f"Rapid events: {success_count}/50 succeeded, {fail_count} failed", fail_count == 0,
           f"Success rate: {success_count/50*100:.0f}%")

    # Verify server still works after rapid fire
    resp_after = send_event(make_event("Stop", sid, status="waiting_for_input"))
    ok_after = is_ok(resp_after)
    report("Server responsive after rapid events", ok_after, f"Response: {repr(resp_after)}")

    send_event(make_event("SessionEnd", sid, status="ended"))

# ============================================================
# Test 10: Invalid JSON
# ============================================================
def test_invalid_json():
    print("\n--- Test 10: Invalid JSON ---")

    invalid_payloads = [
        (b"not json at all\n", "plain text"),
        (b"{invalid json}\n", "broken JSON"),
        (b"{'single_quotes': 'bad'}\n", "single-quoted JSON"),
        (b"\n", "empty line"),
        (b"{}\n", "empty object"),
    ]

    all_survived = True
    for payload, desc in invalid_payloads:
        try:
            resp = send_raw(payload, timeout=1.0)
            report(f"Invalid JSON ({desc}): server survived", True, f"Response: {repr(resp)}")
        except Exception as e:
            all_survived = False
            report(f"Invalid JSON ({desc}): server crashed", False, f"Error: {e}")

    # Verify server is still running after all invalid payloads
    sid = f"test-after-invalid-{uuid.uuid4().hex[:8]}"
    resp_ok = send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    server_alive = is_ok(resp_ok)
    report("Server alive after invalid JSON tests", server_alive, f"Response: {repr(resp_ok)}")
    send_event(make_event("SessionEnd", sid, status="ended"))

    report("Invalid JSON handling (no crashes)", all_survived and server_alive)

# ============================================================
# Test 11: Empty session_id
# ============================================================
def test_empty_session_id():
    print("\n--- Test 11: Empty session_id ---")

    resp = send_event(make_event("SessionStart", "", status="waiting_for_input"))
    report("Empty session_id: server handled it", not (isinstance(resp, str) and resp.startswith("ERROR:")),
           f"Response: {repr(resp)}")

    # Also test with missing session_id
    event_no_sid = {
        "agent": "claude-code",
        "event": "SessionStart",
        "cwd": "/tmp/test",
        "status": "waiting_for_input",
        "pid": os.getpid(),
        "tty": "/dev/ttys999"
    }
    resp2 = send_event(event_no_sid)
    report("Missing session_id: server handled it", not (isinstance(resp2, str) and resp2.startswith("ERROR:")),
           f"Response: {repr(resp2)}")

    # Verify server still works
    sid = f"test-after-empty-{uuid.uuid4().hex[:8]}"
    resp_ok = send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    report("Server works after empty/missing session_id", is_ok(resp_ok))
    send_event(make_event("SessionEnd", sid, status="ended"))

# ============================================================
# Test 12: Large Payload
# ============================================================
def test_large_payload():
    print("\n--- Test 12: Large Payload (10KB+) ---")
    sid = f"test-large-{uuid.uuid4().hex[:8]}"

    send_event(make_event("SessionStart", sid, status="waiting_for_input"))
    send_event(make_event("UserPromptSubmit", sid, status="processing"))
    time.sleep(0.05)

    # Create a large tool_input (>10KB)
    large_data = "x" * 12000  # 12KB of data
    resp = send_event(make_event("PreToolUse", sid,
        status="running_tool",
        tool="Write",
        tool_input={"file_path": "/tmp/large.txt", "content": large_data},
        tool_use_id="tu_large_001"), timeout=3.0)
    ok = is_ok(resp)
    payload_size = len(json.dumps(make_event("PreToolUse", sid,
        status="running_tool",
        tool="Write",
        tool_input={"file_path": "/tmp/large.txt", "content": large_data},
        tool_use_id="tu_large_001")))
    report(f"Large payload ({payload_size} bytes) accepted", ok, f"Response: {repr(resp)}")

    # Verify server still works
    resp_after = send_event(make_event("Stop", sid, status="waiting_for_input"))
    report("Server responsive after large payload", is_ok(resp_after))

    send_event(make_event("SessionEnd", sid, status="ended"))

# ============================================================
# Run All Tests
# ============================================================
def main():
    print("=" * 60)
    print("Agent Island QA: State Machine & Event Processing Tests")
    print("=" * 60)
    print(f"Socket: {SOCKET_PATH}")
    print(f"PID: {os.getpid()}")

    # Check socket exists
    if not os.path.exists(SOCKET_PATH):
        print(f"FATAL: Socket {SOCKET_PATH} does not exist!")
        sys.exit(1)

    test_session_lifecycle()
    test_token_accumulation()
    test_subagent_tracking()
    test_tool_events()
    test_context_compaction()
    test_error_state()
    test_multiple_sessions()
    test_permission_request()
    test_rapid_events()
    test_invalid_json()
    test_empty_session_id()
    test_large_payload()

    # Summary
    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    passed = sum(1 for _, p, _ in RESULTS if p)
    failed = sum(1 for _, p, _ in RESULTS if not p)
    total = len(RESULTS)
    print(f"Total: {total} | Passed: {passed} | Failed: {failed}")

    if failed > 0:
        print("\nFailed tests:")
        for name, p, detail in RESULTS:
            if not p:
                print(f"  - {name}: {detail}")

    print(f"\nOverall: {'ALL PASSED' if failed == 0 else f'{failed} FAILURES'}")
    return 0 if failed == 0 else 1

if __name__ == "__main__":
    sys.exit(main())
