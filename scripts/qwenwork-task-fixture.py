import sqlite3
import sys
import time
from pathlib import Path

database = Path(sys.argv[1])
phase = sys.argv[2]
database.parent.mkdir(parents=True, exist_ok=True)
updated = time.time_ns() // 1_000_000

with sqlite3.connect(database) as conn:
    conn.execute('''
        CREATE TABLE IF NOT EXISTS desk_turns (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            ordinal INTEGER NOT NULL,
            chat_id TEXT NOT NULL,
            sub_chat_id TEXT NOT NULL,
            input_id TEXT,
            assistant_message_ids TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL,
            runtime_epoch TEXT NOT NULL,
            lifecycle TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )
    ''')
    conn.execute('''
        INSERT INTO desk_turns (id, session_id, ordinal, chat_id, sub_chat_id,
                                status, runtime_epoch, created_at, updated_at)
        VALUES ('turn', 'agentbro-windows-fixture', 1, 'chat', 'subchat', ?, 'runtime', ?, ?)
        ON CONFLICT (id) DO UPDATE SET status = excluded.status, updated_at = excluded.updated_at
    ''', (phase, updated, updated))
