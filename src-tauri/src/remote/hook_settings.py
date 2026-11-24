import base64
import fcntl
import json
import os
import pathlib
import shlex
import stat
import sys
import tempfile


def claude_config_root(home):
    configured = os.environ.get("CLAUDE_CONFIG_DIR", "").strip()
    if configured and "," not in configured:
        if configured == "~":
            return home
        path = home / configured[2:] if configured.startswith("~/") else pathlib.Path(configured)
        if path.is_absolute():
            return path
    return home / ".claude"


def agentbro_hook_kind(entry):
    command = entry
    if isinstance(entry, dict):
        if entry.get("type", "command") != "command":
            return None
        command = entry.get("command")
    if not isinstance(command, str):
        return None
    try:
        words = shlex.split(command, comments=True)
    except ValueError:
        return None

    if words and pathlib.PurePosixPath(words[0]).name == "env":
        words.pop(0)
    while words and words[0].partition("=")[0].isidentifier() and "=" in words[0]:
        words.pop(0)
    if not words:
        return None

    executable = pathlib.PurePosixPath(words[0]).name
    script = words[1] if executable.startswith("python") and len(words) > 1 else words[0]
    if ("/" + script).endswith((
        "/.agentbro/remote/hook.py",
        "/.agentbro/remote-hook.py",
        "/.agent-island/remote-hook.py",
    )):
        return "remote"
    if executable in ("agentbro-bridge", "agentbro-bridge.exe") and not command.endswith(" # AgentBro WSL"):
        return "local"
    return None


def hook_group_commands(group):
    if isinstance(group, dict) and isinstance(group.get("hooks"), list):
        return group["hooks"]
    return [group]


def read_hook_settings(path):
    try:
        settings = json.loads(path.read_text(encoding="utf-8-sig") or "{}")
    except FileNotFoundError:
        return {}
    if not isinstance(settings, dict) or not isinstance(settings.get("hooks", {}), dict):
        raise ValueError("settings and hooks must be JSON objects")
    return settings


def remote_hook_status(home, config_path):
    try:
        settings = read_hook_settings(config_path)
    except ValueError:
        return "settingsCorrupted"
    has_remote_hook = any(
        agentbro_hook_kind(entry) == "remote"
        for groups in settings.get("hooks", {}).values()
        if isinstance(groups, list)
        for group in groups
        for entry in hook_group_commands(group)
    )
    if not has_remote_hook:
        return "notInstalled"
    bridge = home / ".agentbro" / "remote" / "hook.py"
    return "installed" if bridge.is_file() else "needsReinstall"


def strip_managed_hooks(hooks):
    removed = 0
    for event, groups in list(hooks.items()):
        if not isinstance(groups, list):
            continue
        kept_groups = []
        for group in groups:
            commands = hook_group_commands(group)
            kept = [entry for entry in commands if agentbro_hook_kind(entry) is None]
            removed += len(commands) - len(kept)
            if len(kept) == len(commands):
                kept_groups.append(group)
            elif kept:
                kept_groups.append(dict(group, hooks=kept))
        if kept_groups or not groups:
            hooks[event] = kept_groups
        else:
            del hooks[event]
    return removed


def write_hook_settings(path, settings):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=str(path.parent), delete=False
        ) as temp:
            temp_path = pathlib.Path(temp.name)
            if path.exists():
                os.fchmod(temp.fileno(), stat.S_IMODE(path.stat().st_mode))
            json.dump(settings, temp, indent=2)
            temp.flush()
            os.fsync(temp.fileno())
        temp_path.replace(path)
    finally:
        if temp_path is not None and temp_path.exists():
            temp_path.unlink()


def update_remote_hooks(home, path, request):
    lock_path = home / ".agentbro" / "remote" / "hook-settings.lock"
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open("a+") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        path = path.resolve()
        settings = read_hook_settings(path)
        hooks = settings.get("hooks", {})
        removed = strip_managed_hooks(hooks)
        if request["operation"] == "remove":
            if removed:
                if not hooks:
                    del settings["hooks"]
                write_hook_settings(path, settings)
            return "hooks removed: {}".format(removed)

        command = " ".join([
            "AGENTBRO_SOCKET=" + shlex.quote(request["socketPath"]),
            "AGENTBRO_HOST_ID=" + shlex.quote(request["hostId"]),
            "AGENTBRO_HOST_NAME=" + shlex.quote(request["hostName"]),
            "AGENTBRO_AGENT=" + shlex.quote(request["source"]),
            "python3 ~/.agentbro/remote/hook.py",
        ])
        for event in request["events"]:
            handler = {"type": "command", "command": command}
            if event["timeout"] is not None:
                handler["timeout"] = event["timeout"]
            group = {"hooks": [handler]}
            if event["matcher"] is not None:
                group["matcher"] = event["matcher"]
            groups = hooks.setdefault(event["name"], [])
            if not isinstance(groups, list):
                raise ValueError("Expected a list for the {} Hook".format(event["name"]))
            groups.append(group)
        settings["hooks"] = hooks
        write_hook_settings(path, settings)
        return "hooks configured: {}".format(len(request["events"]))


def run_hook_settings(encoded_request):
    request = json.loads(base64.b64decode(encoded_request))
    home = pathlib.Path.home().resolve()
    if request["profileId"] == "claude-code":
        path = claude_config_root(home) / "settings.json"
    else:
        path = home / request["configPath"]
    try:
        if request["operation"] == "status":
            print("agentbro_hook_status=" + json.dumps(remote_hook_status(home, path)))
        else:
            print(request["source"], update_remote_hooks(home, path, request))
    except (OSError, ValueError, RuntimeError) as error:
        print("{} {}: {}".format(request["source"], path, error), file=sys.stderr)
        raise SystemExit(1)
