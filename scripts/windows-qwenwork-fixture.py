import json
import sqlite3
import sys
import time
from pathlib import Path


root = Path(sys.argv[1])
mode = sys.argv[2]
now = time.time_ns() // 1_000_000
desk_path = root / "host-agent-responses.db"

if mode == "resume":
    with sqlite3.connect(desk_path) as database:
        updated = database.execute(
            "UPDATE desk_turns SET status = 'in_progress', updated_at = ? WHERE session_id = 'live-desk'",
            (now,),
        )
        if updated.rowcount != 1:
            raise RuntimeError("Did not find the waiting desktop task")
elif mode == "seed":
    live_at = now - 120_000
    completed_at = now - 60_000
    flash_title = "QwenWork Flash smoke task"

    with sqlite3.connect(desk_path) as database:
        database.execute("CREATE TABLE desk_turns (session_id TEXT, status TEXT, ordinal INTEGER, updated_at INTEGER)")
        database.executemany(
            "INSERT INTO desk_turns VALUES (?, 'completed', 1, ?)",
            [(f"desk-{index}", completed_at) for index in range(70)],
        )
        database.execute("INSERT INTO desk_turns VALUES ('live-desk', 'waiting_interaction', 1, ?)", (live_at,))

    flash_root = root / ".harness" / "flash-cli" / "qwenwork"
    flash_root.mkdir(parents=True)
    with sqlite3.connect(flash_root / "qwenwork.db") as database:
        database.executescript("""
            CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, time_updated INTEGER, time_archived INTEGER, parent_id TEXT);
            CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);
        """)
        for index in range(70):
            session_id = f"flash-{index}"
            prompt_id = f"{session_id}-prompt"
            database.execute("INSERT INTO session VALUES (?, 'Earlier task', ?, NULL, NULL)", (session_id, completed_at))
            database.execute(
                "INSERT INTO message VALUES (?, ?, ?, ?, ?)",
                (prompt_id, session_id, completed_at - 1, completed_at - 1, '{"role":"user"}'),
            )
            reply = json.dumps({"role": "assistant", "parentID": prompt_id, "time": {"completed": completed_at}, "finish": "stop"})
            database.execute(
                "INSERT INTO message VALUES (?, ?, ?, ?, ?)",
                (f"{session_id}-reply", session_id, completed_at, completed_at, reply),
            )
        database.execute("INSERT INTO session VALUES ('live-flash', ?, ?, NULL, NULL)", (flash_title, live_at))
        database.execute(
            "INSERT INTO message VALUES ('live-prompt', 'live-flash', ?, ?, ?)",
            (live_at, live_at, '{"role":"user"}'),
        )

    print(json.dumps({
        "agent": "千问办公",
        "flashTitle": flash_title,
        "waiting": "请在千问办公中处理待确认事项",
        "running": "任务进行中",
    }))
else:
    raise ValueError(f"Unknown QwenWork fixture mode: {mode}")
