#!/bin/bash
# Test script for Agent Island HookServer
# Run this while the app is running to verify the full event pipeline.

SOCKET="/tmp/agent-island.sock"
SESSION_ID="test-$(date +%s)"

echo "=== Agent Island Hook Test ==="
echo "Socket: $SOCKET"
echo "Session: $SESSION_ID"
echo ""

# Test 1: SessionStart
echo "--- Test 1: SessionStart ---"
echo "{\"agent\":\"claude-code\",\"event\":\"SessionStart\",\"session_id\":\"$SESSION_ID\",\"cwd\":\"$PWD\",\"status\":\"waiting_for_input\",\"pid\":$$,\"tty\":\"$(tty 2>/dev/null || echo /dev/ttys000)\"}" | nc -U "$SOCKET"
echo "Sent SessionStart. Check the notch panel — you should see a new session."
sleep 2

# Test 2: Processing (UserPromptSubmit)
echo "--- Test 2: UserPromptSubmit ---"
echo "{\"agent\":\"claude-code\",\"event\":\"UserPromptSubmit\",\"session_id\":\"$SESSION_ID\",\"cwd\":\"$PWD\",\"status\":\"processing\",\"pid\":$$}" | nc -U "$SOCKET"
echo "Sent UserPromptSubmit. Session should show 'processing'."
sleep 2

# Test 3: PreToolUse
echo "--- Test 3: PreToolUse ---"
echo "{\"agent\":\"claude-code\",\"event\":\"PreToolUse\",\"session_id\":\"$SESSION_ID\",\"cwd\":\"$PWD\",\"status\":\"running_tool\",\"tool\":\"Read\",\"tool_input\":{\"file_path\":\"/tmp/test.txt\"},\"pid\":$$}" | nc -U "$SOCKET"
echo "Sent PreToolUse (Read). Session should show tool activity."
sleep 2

# Test 4: PostToolUse
echo "--- Test 4: PostToolUse ---"
echo "{\"agent\":\"claude-code\",\"event\":\"PostToolUse\",\"session_id\":\"$SESSION_ID\",\"cwd\":\"$PWD\",\"status\":\"processing\",\"tool\":\"Read\",\"tool_input\":{\"file_path\":\"/tmp/test.txt\"},\"pid\":$$}" | nc -U "$SOCKET"
echo "Sent PostToolUse."
sleep 2

# Test 5: Notification (idle)
echo "--- Test 5: Notification (idle) ---"
echo "{\"agent\":\"claude-code\",\"event\":\"Notification\",\"session_id\":\"$SESSION_ID\",\"cwd\":\"$PWD\",\"status\":\"notification\",\"notification_type\":\"idle_prompt\",\"message\":\"Claude is waiting for your input\",\"pid\":$$}" | nc -U "$SOCKET"
echo "Sent idle notification."
sleep 2

# Test 6: Stop (waiting for input)
echo "--- Test 6: Stop ---"
echo "{\"agent\":\"claude-code\",\"event\":\"Stop\",\"session_id\":\"$SESSION_ID\",\"cwd\":\"$PWD\",\"status\":\"waiting_for_input\",\"pid\":$$}" | nc -U "$SOCKET"
echo "Sent Stop. Session should show idle/waiting."
sleep 2

# Test 7: SessionEnd
echo "--- Test 7: SessionEnd ---"
echo "{\"agent\":\"claude-code\",\"event\":\"SessionEnd\",\"session_id\":\"$SESSION_ID\",\"cwd\":\"$PWD\",\"status\":\"ended\",\"pid\":$$}" | nc -U "$SOCKET"
echo "Sent SessionEnd. Session should be removed."

echo ""
echo "=== Test Complete ==="
echo "If you saw session state changes in the notch panel, the end-to-end pipeline works!"
