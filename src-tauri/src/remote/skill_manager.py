import base64
import datetime
import fcntl
import hashlib
import json
import os
import pathlib
import re
import select
import shutil
import signal
import stat
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request
import uuid
from contextlib import contextmanager
from itertools import count
from os import readlink

MARKER = "__AGENTBRO_REMOTE_SKILL_MANAGER__"
REQUEST = json.loads(base64.b64decode("__AGENTBRO_REQUEST_B64__").decode())
COMMAND = REQUEST.get("command", "")
ARGS = REQUEST.get("args") or {}
HOME = pathlib.Path.home().resolve()
STATE_PATH = HOME / ".agentbro" / "skill-manager-remote.json"
FIXED_CENTER_PATH = HOME / ".agentbro" / "skills"
SHARED_SKILLS_AGENT_ID = "agents"
DEFAULT_PACK_NAME = "全量技能包"
SHARED_SKILL_CONSUMERS = ("codex", "kimi", "openclaw")
MCP_INSPECTION_MAX_PAGES = 10
MCP_INSPECTION_MAX_ITEMS = 500
MCP_INSPECTION_CATALOG_TIMEOUT = 15
MCP_SSE_LINE_END = re.compile(br"\r\n?|\n")
STATE_MUTATING_COMMANDS = {
    "skill_manager_bootstrap",
    "skill_manager_init",
    "skill_manager_refresh",
    "skill_manager_update_settings",
    "execute_add_center_skill",
    "execute_marketplace_skill_batch",
    "import_github_repo_skills",
    "execute_delete_center_skill",
    "execute_delete_center_skills",
    "execute_adopt_agent_skill",
    "execute_adopt_agent_skills",
    "takeover_center_agent_skills",
    "delete_unmanaged_agent_skill",
    "delete_unmanaged_agent_skills",
    "delete_skill_target_distribution",
    "delete_skill_target_distributions",
    "execute_sync_copy_target",
    "execute_fix_diagnosis_issue",
    "execute_upsert_skill_pack",
    "execute_import_skill_pack_manifest",
    "restore_skill_pack_member",
    "execute_delete_skill_pack",
    "execute_apply_skill_pack",
    "execute_sync_skill_pack_to_agents",
    "execute_remove_skill_pack_from_agent",
    "execute_remove_skill_from_pack",
    "add_skill_project_v2",
    "remove_skill_project_v2",
    "get_skill_project_detail_v2",
    "scan_skill_project_v2",
    "execute_move_direct_skill_to_pack",
    "write_agent_config_file_v2",
    "set_plugin_enabled_v2",
    "save_mcp_server_cmd",
    "set_mcp_server_enabled_cmd",
    "delete_mcp_server_v2_cmd",
    "add_custom_agent",
    "update_custom_agent",
    "remove_custom_agent",
}
STATE_LOCK = None
if COMMAND in STATE_MUTATING_COMMANDS:
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    STATE_LOCK = (STATE_PATH.parent / "skill-manager-remote.lock").open("a+")
    fcntl.flock(STATE_LOCK.fileno(), fcntl.LOCK_EX)
AGENTS = [
    {
        "id": "claude-code",
        "displayName": "Claude Code",
        "iconKey": "claude-code",
        "skills": ".claude/skills",
        "config": ".claude/settings.json",
        "mcp": ".claude.json",
        "plugins": ".claude/plugins",
        "commands": ["claude"],
    },
    {
        "id": "codex",
        "displayName": "Codex",
        "iconKey": "codex",
        "skills": ".codex/skills",
        "config": ".codex/config.toml",
        "mcp": ".codex/config.toml",
        "plugins": ".codex/plugins",
        "commands": ["codex"],
    },
    {
        "id": "gemini",
        "displayName": "Gemini CLI",
        "iconKey": "gemini",
        "skills": ".gemini/skills",
        "config": ".gemini/settings.json",
        "mcp": ".gemini/settings.json",
        "plugins": ".gemini/extensions",
        "commands": ["gemini"],
    },
    {
        "id": "cursor",
        "displayName": "Cursor",
        "iconKey": "cursor",
        "skills": ".cursor/skills",
        "config": ".cursor/settings.json",
        "mcp": ".cursor/mcp.json",
        "plugins": ".cursor/extensions",
        "commands": ["cursor", "cursor-agent"],
    },
    {
        "id": "opencode",
        "displayName": "OpenCode",
        "iconKey": "opencode",
        "skills": ".opencode/skills",
        "config": ".config/opencode/opencode.json",
        "mcp": ".config/opencode/opencode.json",
        "plugins": ".opencode/plugins",
        "commands": ["opencode"],
    },
    {
        "id": "openclaw",
        "displayName": "OpenClaw",
        "iconKey": "openclaw",
        "skills": ".openclaw/workspace/skills",
        "config": ".openclaw/openclaw.json",
        "mcp": ".openclaw/openclaw.json",
        "plugins": ".openclaw/extensions",
        "commands": ["openclaw"],
    },
    {
        "id": "qclaw",
        "displayName": "QClaw",
        "iconKey": "qclaw",
        "skills": ".qclaw/skills",
        "config": ".qclaw/settings.json",
        "mcp": ".qclaw/mcp.json",
        "plugins": ".qclaw/plugins",
        "commands": ["qclaw"],
    },
    {
        "id": "easyclaw",
        "displayName": "EasyClaw",
        "iconKey": "easyclaw",
        "skills": ".easyclaw/skills",
        "config": ".easyclaw/settings.json",
        "mcp": ".easyclaw/mcp.json",
        "plugins": ".easyclaw/plugins",
        "commands": ["easyclaw"],
    },
    {
        "id": "copilot",
        "displayName": "GitHub Copilot",
        "iconKey": "copilot",
        "skills": ".copilot/skills",
        "config": ".copilot/config.json",
        "mcp": ".copilot/mcp.json",
        "plugins": ".copilot/plugins",
        "commands": ["github-copilot", "copilot"],
    },
    {
        "id": "qwen",
        "displayName": "Qwen Code",
        "iconKey": "qwen",
        "skills": ".qwen/skills",
        "config": ".qwen/settings.json",
        "mcp": ".qwen/settings.json",
        "plugins": ".qwen/plugins",
        "commands": ["qwen"],
    },
    {
        "id": "kimi",
        "displayName": "Kimi Code",
        "iconKey": "kimi",
        "skills": ".kimi/skills",
        "config": ".kimi/config.json",
        "mcp": ".kimi/mcp.json",
        "plugins": ".kimi/plugins",
        "commands": ["kimi", "kimi-cli"],
    },
    {
        "id": "windsurf",
        "displayName": "Windsurf",
        "iconKey": "windsurf",
        "skills": ".windsurf/skills",
        "config": ".windsurf/settings.json",
        "mcp": ".windsurf/mcp.json",
        "plugins": ".windsurf/plugins",
        "commands": ["windsurf"],
    },
    {
        "id": "aider",
        "displayName": "Aider",
        "iconKey": "aider",
        "skills": ".aider/skills",
        "config": ".aider.conf.yml",
        "mcp": ".aider/mcp.json",
        "plugins": ".aider/plugins",
        "commands": ["aider"],
    },
]


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def unlink_if_exists(path):
    try:
        path.unlink()
    except FileNotFoundError:
        pass


def has_surrogate(value):
    return any(0xD800 <= ord(character) <= 0xDFFF for character in str(value))


def json_safe(value):
    if isinstance(value, str):
        return "".join(
            "\ufffd" if 0xD800 <= ord(character) <= 0xDFFF else character
            for character in value
        )
    if isinstance(value, list):
        return [json_safe(item) for item in value]
    if isinstance(value, tuple):
        return [json_safe(item) for item in value]
    if isinstance(value, dict):
        return {
            json_safe(key) if isinstance(key, str) else key: json_safe(item)
            for key, item in value.items()
        }
    return value


def default_state():
    return {
        "settings": {
            "centerPath": str(FIXED_CENTER_PATH),
            "sqlitePath": str(STATE_PATH),
            "defaultDistributeMode": "link",
            "linkFailPolicy": "ask",
            "startupScan": True,
            "showUnmanaged": True,
            "autoSyncSkillPacks": True,
        },
        "sources": {},
        "packs": {},
        "packTargetClaims": {},
        "projects": {},
        "customAgents": {},
    }


def load_state():
    state = default_state()
    if STATE_PATH.is_file():
        try:
            saved = json.loads(STATE_PATH.read_text())
            for key in ("settings", "sources", "packs", "packTargetClaims", "projects", "customAgents"):
                if isinstance(saved.get(key), dict):
                    state[key].update(saved[key])
        except Exception:
            pass
    return state


STATE = load_state()
CONFIGURED_CENTER_PATH = str(STATE["settings"].get("centerPath") or FIXED_CENTER_PATH)
STATE["settings"]["centerPath"] = str(FIXED_CENTER_PATH)


def write_json_atomically(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            dir=path.parent,
            prefix=f".{path.name}.",
            suffix=".tmp",
            delete=False,
        ) as temp:
            temp_path = pathlib.Path(temp.name)
            json.dump(value, temp, ensure_ascii=False, indent=2)
            temp.flush()
            os.fsync(temp.fileno())
        temp_path.replace(path)
    finally:
        if temp_path is not None:
            unlink_if_exists(temp_path)


def save_state():
    write_json_atomically(STATE_PATH, STATE)


def expand_path(value):
    raw = str(value or "")
    if raw == "~":
        return HOME
    if raw.startswith("~/"):
        return HOME / raw[2:]
    return pathlib.Path(raw).expanduser()


def center_path():
    path = FIXED_CENTER_PATH
    STATE["settings"]["centerPath"] = str(path)
    if path.resolve(strict=False) == pathlib.Path("/"):
        raise ValueError("The remote filesystem root cannot be used as the Skill center")
    path.mkdir(parents=True, exist_ok=True)
    return path


def safe_id(value):
    value = str(value or "")
    if not re.fullmatch(r"[A-Za-z0-9._-]+", value):
        raise ValueError("Invalid identifier")
    return value


def safe_home_path(value, allow_missing=True):
    path = expand_path(value)
    resolved = path.resolve(strict=False)
    inside_home = resolved == HOME or HOME in resolved.parents
    configured_center = FIXED_CENTER_PATH.resolve(strict=False)
    inside_center = resolved == configured_center or configured_center in resolved.parents
    inside_project = any(
        resolved == pathlib.Path(project["rootPath"]).resolve(strict=False)
        or pathlib.Path(project["rootPath"]).resolve(strict=False) in resolved.parents
        for project in STATE.get("projects", {}).values()
    )
    if not inside_home and not inside_center and not inside_project:
        raise ValueError("Remote path is outside the managed home and project directories")
    if not allow_missing and not path.exists():
        raise FileNotFoundError(str(path))
    return path


def browse_skill_sources(value):
    directory = safe_home_path(value or "~", allow_missing=False).resolve()
    if not directory.is_dir():
        raise ValueError("Remote source path is not a directory")
    entries = []
    try:
        children = list(directory.iterdir())
    except PermissionError as error:
        raise ValueError("Remote source directory is not readable") from error
    for child in children:
        try:
            child_name = child.name
            child_path = str(child)
            if has_surrogate(child_name) or has_surrogate(child_path):
                continue
            safe_home_path(child_path, allow_missing=False)
            is_directory = child.is_dir()
            is_archive = child.is_file() and child.suffix.lower() == ".zip"
            if not is_directory and not is_archive:
                continue
            entries.append({
                "name": child_name,
                "path": child_path,
                "entryType": "directory" if is_directory else "archive",
                "hasSkillManifest": is_directory and (child / "SKILL.md").is_file(),
            })
        except (FileNotFoundError, PermissionError, ValueError):
            continue
    entries.sort(key=lambda item: (
        item["entryType"] != "directory",
        item["name"].lower(),
    ))
    parent_path = None
    if directory.resolve(strict=False) != HOME:
        try:
            parent = safe_home_path(str(directory.parent), allow_missing=False)
            if parent.is_dir():
                parent_path = str(parent)
        except (FileNotFoundError, PermissionError, ValueError):
            pass
    return {
        "path": str(directory),
        "parentPath": parent_path,
        "entries": entries,
    }


def project_root_path(value):
    path = expand_path(value)
    resolved = path.resolve(strict=False)
    if resolved == pathlib.Path("/"):
        raise ValueError("The remote filesystem root cannot be added as a project")
    if not resolved.is_dir():
        raise FileNotFoundError(str(resolved))
    return resolved


def custom_agent_spec(record):
    root = safe_home_path(record["configDir"]).absolute()
    settings = safe_home_path(record.get("settingsFile") or root / "settings.json").absolute()
    return {
        "id": record["id"],
        "displayName": record["displayName"],
        "iconKey": record.get("icon") or "custom",
        "commands": [],
        "configDir": root,
        "config": settings,
        "mcp": safe_home_path(record.get("mcpConfig") or settings).absolute(),
        "plugins": safe_home_path(record.get("pluginDir") or root / "plugins" / "cache").absolute(),
        "skills": safe_home_path(record["skillsDir"]).absolute(),
        "category": record.get("category") or (
            "claude-compatible" if record.get("icon") == "claude-code" else "custom"
        ),
    }


def agent_spec(agent_id):
    built_in = next((item for item in AGENTS if item["id"] == agent_id), None)
    if built_in or agent_id == SHARED_SKILLS_AGENT_ID:
        return built_in
    record = STATE["customAgents"].get(agent_id)
    if record:
        try:
            return custom_agent_spec(record)
        except (OSError, ValueError, RuntimeError):
            return None
    return None


def management_agent_specs():
    yield from AGENTS
    for agent_id in STATE["customAgents"]:
        spec = agent_spec(agent_id)
        if spec and "configDir" in spec:
            yield spec


def is_claude_agent(agent_id):
    if agent_id == "claude-code":
        return True
    spec = agent_spec(agent_id)
    return bool(spec and spec.get("category") == "claude-compatible")


def openclaw_state_path():
    configured = os.environ.get("OPENCLAW_STATE_DIR")
    if configured:
        return expand_path(configured)
    profile = os.environ.get("OPENCLAW_PROFILE")
    if profile and profile != "default":
        return HOME / (".openclaw-" + profile)
    return HOME / ".openclaw"


def agent_config_path(agent_id):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + str(agent_id))
    if agent_id == "openclaw":
        configured = os.environ.get("OPENCLAW_CONFIG_PATH")
        return expand_path(configured) if configured else openclaw_state_path() / "openclaw.json"
    if agent_id == "codex" and os.environ.get("CODEX_HOME"):
        return expand_path(os.environ["CODEX_HOME"]) / "config.toml"
    if agent_id == "claude-code" and os.environ.get("CLAUDE_CONFIG_DIR"):
        return expand_path(os.environ["CLAUDE_CONFIG_DIR"]) / "settings.json"
    return HOME / spec["config"]


def agent_mcp_path(agent_id):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + str(agent_id))
    return agent_config_path(agent_id) if agent_id in ("openclaw", "codex") else HOME / spec["mcp"]


def agent_plugin_path(agent_id):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + str(agent_id))
    if agent_id == "openclaw":
        return openclaw_state_path() / "extensions"
    if agent_id in ("codex", "claude-code"):
        return agent_config_path(agent_id).parent / "plugins" / "cache"
    return HOME / spec["plugins"]


def openclaw_workspace_path():
    config_path = agent_config_path("openclaw")
    try:
        config = json.loads(config_path.read_text(errors="replace"))
        configured = config.get("agents", {}).get("defaults", {}).get("workspace")
        if isinstance(configured, str) and configured.strip():
            return expand_path(configured.strip())
    except Exception:
        pass
    return openclaw_state_path() / "workspace"


def agent_skills_path(agent_id):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + str(agent_id))
    if agent_id == "openclaw":
        return openclaw_workspace_path() / "skills"
    return HOME / spec["skills"]


def agent_binary(spec):
    for command in spec["commands"]:
        binary = shutil.which(command)
        if binary:
            return binary
    if spec["id"] != "openclaw":
        return None
    candidates = [
        openclaw_state_path() / "bin" / "openclaw",
        HOME / ".npm-global" / "bin" / "openclaw",
        HOME / ".local" / "bin" / "openclaw",
        HOME / ".local" / "share" / "pnpm" / "openclaw",
        HOME / ".bun" / "bin" / "openclaw",
        HOME / ".volta" / "bin" / "openclaw",
    ]
    candidates.extend(sorted(
        (HOME / ".nvm" / "versions" / "node").glob("*/bin/openclaw"),
        key=str,
        reverse=True,
    ))
    candidates.extend(sorted(
        (HOME / ".local" / "share" / "fnm" / "node-versions").glob(
            "*/installation/bin/openclaw"
        ),
        key=str,
        reverse=True,
    ))
    return next(
        (str(path) for path in candidates if path.is_file() and os.access(str(path), os.X_OK)),
        None,
    )


def agent_installed(spec, binary=None):
    if binary:
        return True
    if "configDir" in spec:
        return spec["configDir"].is_dir() or spec["skills"].parent.is_dir()
    if spec["id"] == "openclaw" and openclaw_state_path().exists():
        return True
    return agent_skills_path(spec["id"]).exists()


def frontmatter(skill_file, fallback):
    result = {"name": fallback, "description": ""}
    try:
        text = skill_file.read_text(errors="replace")[:65536]
    except Exception:
        return result
    if not text.startswith("---"):
        return result
    parts = text.split("---", 2)
    if len(parts) < 3:
        return result
    for line in parts[1].splitlines():
        match = re.match(r"^(name|description)\s*:\s*(.*)$", line.strip())
        if not match:
            continue
        value = match.group(2).strip().strip("'\"")
        if match.group(1) == "name" and value:
            result["name"] = value
        elif match.group(1) == "description" and value not in (">", "|", ">-", "|-"):
            result["description"] = value
    return result


def hash_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(65536)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def hash_dir(path):
    digest = hashlib.sha256()
    try:
        files = sorted(
            item for item in path.rglob("*")
            if item.is_file() and ".git" not in item.parts
        )
        for item in files:
            digest.update(str(item.relative_to(path)).encode())
            digest.update(hash_file(item).encode())
        return digest.hexdigest()
    except Exception:
        return ""


def add_pack_content_file(digest, relative, source):
    with source.open("rb") as handle:
        size = 0
        crlf = 0
        binary = False
        previous = b""
        for chunk in iter(lambda: handle.read(65536), b""):
            size += len(chunk)
            binary = binary or b"\0" in chunk
            crlf += chunk.count(b"\r\n") + (previous == b"\r" and chunk.startswith(b"\n"))
            previous = chunk[-1:]
        normalize = not binary and crlf > 0
        expected_size = size - crlf if normalize else size
        digest.update(f"{relative}\0{expected_size}\0".encode())

        handle.seek(0)
        pending = b""
        written = 0
        for chunk in iter(lambda: handle.read(65536), b""):
            if normalize:
                chunk = pending + chunk
                pending = b"\r" if chunk.endswith(b"\r") else b""
                if pending:
                    chunk = chunk[:-1]
                chunk = chunk.replace(b"\r\n", b"\n")
            digest.update(chunk)
            written += len(chunk)
        digest.update(pending)
        if written + len(pending) != expected_size:
            raise ValueError("A Skill file changed while it was being verified")


def valid_pack_component(name):
    return bool(name) and not (
        name.endswith((".", " "))
        or any(ord(ch) < 32 or 127 <= ord(ch) <= 159 or ch in '<>:"/\\|?*' for ch in name)
        or re.fullmatch(r"CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9]", name.split(".", 1)[0], re.I | re.ASCII)
    )


def pack_content_fingerprint(directory, staged=None, local_import_layout=False):
    root = directory.resolve(strict=True)
    if not stat.S_ISREG((root / "SKILL.md").lstat().st_mode):
        raise ValueError("The installed path is not a Skill directory")
    files = {}

    def collect(directory, prefix, ancestors):
        for entry in directory.iterdir():
            name = entry.name
            if has_surrogate(name):
                raise ValueError("Skill file names must be UTF-8")
            if local_import_layout and (
                entry.is_symlink() or name in (
                    ".git", ".DS_Store", "node_modules", "target", "__pycache__", ".idea",
                    ".venv", "venv", "output",
                ) or name.endswith((".tmp", ".swp"))
            ):
                continue
            source = entry
            if entry.is_symlink():
                try:
                    source = entry.resolve(strict=True)
                except (OSError, RuntimeError):
                    continue
                if source != root and root not in source.parents:
                    continue
            mode = source.stat().st_mode
            if stat.S_ISDIR(mode) and name in (".git", "__pycache__", "__pypackages__"):
                continue
            if not valid_pack_component(name) or name.lower() == ".git":
                raise ValueError("Unsupported Skill file name: " + name)
            relative = prefix + name
            if stat.S_ISDIR(mode) and source not in ancestors:
                collect(source, relative + "/", ancestors | {source})
            elif stat.S_ISREG(mode):
                files[relative] = source

    collect(root, "", {root})
    digest = hashlib.sha256()
    for relative, file in sorted(files.items()):
        if staged is not None:
            target = staged / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(file, target)
            file = target
        add_pack_content_file(digest, relative, file)
    return digest.hexdigest()


def valid_pack_git_source(source):
    if (not isinstance(source, dict) or set(source) != {"repository", "commit", "skillPath"}
            or not all(isinstance(value, str) for value in source.values())):
        return False
    repository = source["repository"]
    parts = repository.split("/")
    return (
        len(parts) == 2 and all(re.fullmatch(r"[A-Za-z0-9._-]+", part) and not part.endswith(".") for part in parts)
        and not parts[1].endswith(".git")
        and re.fullmatch(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})", source["commit"])
        and source["skillPath"].split("/")[-1] == "SKILL.md"
        and all(valid_pack_component(part) and part.lower() != ".git" for part in source["skillPath"].split("/"))
    )


def pack_git_source_from_record(record):
    uri, commit = record.get("sourceUri"), record.get("sourceRef")
    if not isinstance(uri, str) or not isinstance(commit, str):
        return None
    prefix = "https://github.com/"
    encoded = uri.startswith(prefix)
    if not encoded:
        prefix = "github:"
    if not uri.startswith(prefix) or "?" in uri or "#" in uri:
        return None
    parts = uri[len(prefix):].rstrip("/").split("/")
    if len(parts) < 2:
        return None
    owner, repo = parts[:2]
    repo = repo[:-4] if repo.endswith(".git") else repo
    path = parts[2:]
    if path and path[0] == "tree":
        if len(path) < 2 or not path[1]:
            return None
        path = path[2:]
    if encoded:
        try:
            if any(re.search(r"%(?![0-9a-fA-F]{2})", part) for part in path):
                return None
            path = [urllib.parse.unquote(part, errors="strict") for part in path]
        except UnicodeError:
            return None
        if any("/" in part for part in path):
            return None
    if not path or path[-1] != "SKILL.md":
        path.append("SKILL.md")
    source = {
        "repository": owner + "/" + repo,
        "commit": commit.lower(),
        "skillPath": "/".join(path),
    }
    return source if valid_pack_git_source(source) else None


def skill_pack_fingerprints(skill_ids, include_sources=False):
    paths = {path.name: path for path in center_path().iterdir()}
    items = []
    for skill_id in skill_ids:
        entry = {"skillId": skill_id, "missing": False, "fingerprint": None}
        path = paths.get(skill_id)
        try:
            if path is None:
                raise FileNotFoundError()
            (path / "SKILL.md").lstat()
            entry["fingerprint"] = pack_content_fingerprint(path)
        except (FileNotFoundError, NotADirectoryError):
            entry["missing"] = True
        except (OSError, RuntimeError, ValueError):
            pass
        if (include_sources and entry["fingerprint"]
                and valid_pack_component(skill_id) and not skill_id.startswith((".", " "))):
            source = pack_git_source_from_record(STATE["sources"].get(skill_id, {}))
            if source:
                entry["source"] = source
        items.append(entry)
    return items


def validate_reviewable_skill_pack(manifest):
    if manifest.get("version") not in (2, 3) or manifest.get("format") != "agentbro-skill-pack":
        raise ValueError("A content review requires Skill Pack version 2 or later")
    ids = manifest["skillIds"]
    expected = manifest.get("skillFingerprints", {})
    if (not manifest["name"].strip() or len(set(ids)) != len(ids)
            or any(key not in ids or not re.fullmatch(r"[0-9a-f]{64}", value) for key, value in expected.items())):
        raise ValueError("Invalid Skill Pack manifest")
    sources = manifest.get("skillSources", {})
    if manifest["version"] < 3 and sources:
        raise ValueError("Pinned GitHub sources require Skill Pack version 3")
    for skill_id, source in sources.items():
        if (skill_id not in expected or not valid_pack_component(skill_id)
                or skill_id.startswith((".", " ")) or not valid_pack_git_source(source)):
            raise ValueError(f"Invalid pinned GitHub source for '{skill_id}'")


def restore_pack_skill(manifest, skill_id):
    validate_reviewable_skill_pack(manifest)
    source = manifest.get("skillSources", {}).get(skill_id)
    if not source:
        raise ValueError("The Skill has no pinned GitHub source")
    center = center_path().resolve()
    destination = center / skill_id
    if path_present(destination):
        raise ValueError("The Skill already exists in the selected library")
    repository = "https://github.com/" + source["repository"] + ".git"
    commit = source["commit"]
    checkout, temporary = clone_repository(repository, commit)
    try:
        if run_repository_git(checkout, "rev-parse", "HEAD").lower() != commit.lower():
            raise ValueError("Git did not return the recorded Skill commit")
        directory = repository_relative_path(checkout.resolve(), source["skillPath"]).parent
        expected = manifest["skillFingerprints"][skill_id]
        with tempfile.TemporaryDirectory(prefix=".agentbro-skill-import-", dir=str(center)) as staging:
            staged = pathlib.Path(staging) / "skill"
            # The local library filters some entries that SSH imports retain.
            if pack_content_fingerprint(directory, staged, local_import_layout=True) != expected:
                shutil.rmtree(staged)
                pack_content_fingerprint(directory, staged)
            if pack_content_fingerprint(staged) != expected:
                raise ValueError("Skill content does not match the recorded fingerprint")
            try:
                destination.mkdir()
            except FileExistsError:
                raise ValueError("The Skill already exists in the selected library")
            try:
                staged.rename(destination)
            except OSError:
                destination.rmdir()
                raise
            relative = pathlib.PurePosixPath(source["skillPath"]).parent.as_posix()
            STATE["sources"][skill_id] = {
                "sourceType": "github",
                "sourceUri": repository_skill_source(repository, commit, relative),
                "sourceRef": commit,
                "importedFromAgent": None,
                "importedFromPath": None,
                "installedVia": "remote_git",
                "createdAt": now(),
                "updatedAt": now(),
            }
            try:
                save_state()
            except OSError:
                destination.rename(staged)
                raise
    finally:
        temporary.cleanup()


def import_reviewed_skill_pack(manifest, reviewed, accept_local_copies):
    validate_reviewable_skill_pack(manifest)
    ids = manifest["skillIds"]
    expected = manifest.get("skillFingerprints", {})
    current = skill_pack_fingerprints(ids)
    if current != reviewed:
        raise ValueError("The selected library changed since review. Recheck the Skill Pack.")
    for item in current:
        skill_id = item["skillId"]
        if item["missing"]:
            raise ValueError(f"Skill '{skill_id}' is missing from the center library.")
        if not item["fingerprint"]:
            raise ValueError(f"Could not verify Skill '{skill_id}' in the center library.")
        if accept_local_copies is not True and expected.get(skill_id) != item["fingerprint"]:
            raise ValueError(f"Confirm the local copy of Skill '{skill_id}' before importing.")
    return upsert_pack({**manifest, "id": ""})



def skill_directories(root):
    if not root.is_dir():
        return []
    try:
        return [
            entry for entry in sorted(root.iterdir(), key=lambda item: item.name.lower())
            if entry.is_dir() and (entry / "SKILL.md").is_file()
        ]
    except Exception:
        return []


def migrate_configured_center():
    source = expand_path(CONFIGURED_CENTER_PATH).resolve(strict=False)
    target = FIXED_CENTER_PATH.resolve(strict=False)
    if source == target or not source.is_dir():
        return
    target.mkdir(parents=True, exist_ok=True)
    for skill in skill_directories(source):
        destination = target / skill.name
        if destination.exists() or destination.is_symlink():
            continue
        try:
            shutil.copytree(skill, destination, symlinks=True)
        except (FileExistsError, FileNotFoundError, PermissionError):
            continue


def remove_path(path):
    if path.is_symlink() or path.is_file():
        unlink_if_exists(path)
    elif path.is_dir():
        shutil.rmtree(path)


def path_present(path):
    path = pathlib.Path(path)
    return path.exists() or path.is_symlink()


def verified_skill_hash(path, label):
    path = pathlib.Path(path)
    try:
        if not path.is_dir() or not (path / "SKILL.md").is_file():
            raise ValueError(label + " is not a readable Skill directory")
        result = hash_dir(path)
    except (OSError, RuntimeError) as error:
        raise ValueError(label + " could not be verified") from error
    if not result:
        raise ValueError(label + " could not be verified")
    return result


def independent_center_skill_hash(path):
    path = pathlib.Path(path)
    try:
        if (
            FIXED_CENTER_PATH.is_symlink()
            or normalized_lexical_path(path.parent)
            != normalized_lexical_path(FIXED_CENTER_PATH)
            or path.is_symlink()
            or not path.is_dir()
            or (path / "SKILL.md").is_symlink()
        ):
            return None
        if any(item.is_symlink() for item in path.rglob("*")):
            return None
        return verified_skill_hash(path, "Center Skill")
    except (OSError, RuntimeError, ValueError):
        return None


def atomic_replace_skill_directory(
    source, destination, after_replace=None, preserve_symlinks=False, link=False
):
    source = pathlib.Path(source)
    destination = pathlib.Path(destination)
    expected_hash = verified_skill_hash(source, "Source Skill")
    resolved_source = source.resolve()
    resolved_parent = destination.parent.resolve()
    if resolved_source == resolved_parent or resolved_source in resolved_parent.parents:
        raise ValueError("Source Skill overlaps the staging directory")
    if link and paths_overlap(source, destination):
        raise ValueError("Center Skill overlaps the Agent copy")
    if preserve_symlinks and not destination.is_symlink() and paths_overlap(source, destination):
        raise ValueError("Source Skill overlaps its destination")
    destination.parent.mkdir(parents=True, exist_ok=True)
    recovery_root = pathlib.Path(tempfile.mkdtemp(
        prefix=".shared-skill-replace-",
        dir=str(destination.parent),
    ))
    staged = recovery_root / "staged"
    backup = recovery_root / "backup"
    failed_replacement = recovery_root / "failed-replacement"
    keep_recovery = False
    had_destination = path_present(destination)
    try:
        if link:
            staged.symlink_to(source, target_is_directory=True)
        else:
            shutil.copytree(source, staged, symlinks=preserve_symlinks)
        if (
            verified_skill_hash(staged, "Staged Skill") != expected_hash
            or verified_skill_hash(source, "Source Skill") != expected_hash
        ):
            raise ValueError("Source Skill changed while its replacement was staged")
        if had_destination:
            os.replace(str(destination), str(backup))
        activated = False
        try:
            os.replace(str(staged), str(destination))
            activated = True
            if verified_skill_hash(destination, "Replaced Skill") != expected_hash:
                raise ValueError("Replaced Skill failed final verification")
            if after_replace is not None:
                after_replace()
        except Exception as replace_error:
            rollback_errors = []
            if activated and path_present(destination):
                try:
                    os.replace(str(destination), str(failed_replacement))
                except Exception as error:
                    rollback_errors.append("new destination could not be retained: " + str(error))
            if had_destination:
                if path_present(destination):
                    rollback_errors.append("old destination is blocked by the new destination")
                elif path_present(backup):
                    try:
                        os.replace(str(backup), str(destination))
                    except Exception as error:
                        rollback_errors.append("old destination could not be restored: " + str(error))
                else:
                    rollback_errors.append("old destination backup is missing")
            elif path_present(destination):
                rollback_errors.append("new destination could not be rolled back")
            source_restored = False
            try:
                source_restored = (
                    verified_skill_hash(source, "Source Skill") == expected_hash
                )
            except ValueError:
                pass
            if rollback_errors or not source_restored:
                keep_recovery = True
                detail = "; ".join(rollback_errors)
                if not source_restored:
                    detail = (detail + "; " if detail else "") + "source requires recovery"
                raise ValueError(
                    "Atomic Skill replacement failed. Recovery artifacts retained at "
                    + str(recovery_root)
                    + ": "
                    + str(replace_error)
                    + ("; " + detail if detail else "")
                ) from replace_error
            raise
        if path_present(backup):
            try:
                remove_path(backup)
            except Exception as cleanup_error:
                keep_recovery = True
                raise ValueError(
                    "Atomic Skill replacement succeeded but its old destination backup "
                    "could not be cleaned. Recovery artifacts retained at "
                    + str(recovery_root)
                    + ": "
                    + str(cleanup_error)
                ) from cleanup_error
        return expected_hash
    finally:
        if recovery_root.exists() and not keep_recovery:
            try:
                shutil.rmtree(recovery_root)
            except OSError:
                pass


def shared_skills_path():
    return HOME / ".agents" / "skills"


def normalized_lexical_path(path):
    return pathlib.Path(os.path.abspath(os.path.normpath(str(path))))


def paths_overlap(left, right):
    left = pathlib.Path(left)
    right = pathlib.Path(right)
    for normalized_left, normalized_right in (
        (normalized_lexical_path(left), normalized_lexical_path(right)),
        (left.resolve(strict=False), right.resolve(strict=False)),
    ):
        if (
            normalized_left == normalized_right
            or normalized_left in normalized_right.parents
            or normalized_right in normalized_left.parents
        ):
            return True
    return False


def shared_skill_mutation_path(path):
    path = pathlib.Path(path)
    shared_root = shared_skills_path()
    shared_parent = shared_root.parent
    if not path.is_absolute() or path.name in ("", ".", ".."):
        raise ValueError("Shared Skill path must identify a leaf below ~/.agents/skills")
    if shared_parent.is_symlink() or shared_root.is_symlink():
        raise ValueError("Refusing to mutate a shared Skill through a symbolic-link parent")
    if FIXED_CENTER_PATH.is_symlink() or paths_overlap(FIXED_CENTER_PATH, shared_root):
        raise ValueError("Refusing to mutate a shared Skill through an unsafe center root")
    normalized_root = shared_root.resolve(strict=False)
    normalized_path = path.parent.resolve(strict=False) / path.name
    if normalized_path == normalized_root:
        raise ValueError("Shared Skill path must identify a leaf below ~/.agents/skills")
    if normalized_root not in normalized_path.parents:
        raise ValueError("Shared Skill path is outside the trusted ~/.agents/skills root")
    return path


def center_symlink_depends_on(path, dependency):
    path = normalized_lexical_path(path)
    dependency = normalized_lexical_path(dependency)
    current = path
    visited = set()
    while current.is_symlink():
        key = str(current)
        if key in visited:
            break
        visited.add(key)
        target = pathlib.Path(getattr(os, "readlink")(str(current)))
        if not target.is_absolute():
            target = current.parent / target
        target = normalized_lexical_path(target)
        if target == dependency or dependency in target.parents:
            return True
        current = target
    try:
        return path.resolve(strict=True) == dependency.resolve(strict=True)
    except (FileNotFoundError, OSError, RuntimeError):
        return False


def center_symlink_dependents(source):
    center = center_path()
    try:
        entries = list(center.iterdir())
    except (FileNotFoundError, PermissionError) as error:
        raise ValueError("The center library cannot be inspected safely") from error
    return sorted(
        (
            entry for entry in entries
            if entry.is_symlink() and center_symlink_depends_on(entry, source)
        ),
        key=lambda entry: entry.name.lower(),
    )


def detach_center_symlink_dependents(source):
    source = pathlib.Path(source)
    dependents = center_symlink_dependents(source)
    if not dependents:
        return
    source_hash = verified_skill_hash(source, "Shared Skill")
    expected_dependents = []
    for dependent in dependents:
        if (
            not dependent.is_symlink()
            or not center_symlink_depends_on(dependent, source)
        ):
            raise ValueError("A dependent center Skill changed before it was detached")
        expected_dependents.append((
            dependent,
            verified_skill_hash(dependent, "Dependent center Skill"),
        ))
    for dependent, expected_hash in expected_dependents:
        if (
            not dependent.is_symlink()
            or verified_skill_hash(dependent, "Dependent center Skill") != expected_hash
            or verified_skill_hash(source, "Shared Skill") != source_hash
        ):
            raise ValueError("A dependent center Skill changed before it was detached")
        atomic_replace_skill_directory(dependent, dependent)
        if (
            dependent.is_symlink()
            or verified_skill_hash(dependent, "Detached center Skill") != expected_hash
        ):
            raise ValueError("A detached center Skill failed final verification")
    if (
        verified_skill_hash(source, "Shared Skill") != source_hash
        or center_symlink_dependents(source)
    ):
        raise ValueError("Not all dependent center Skills could be detached")


def prepare_shared_target_write(path, center_source, replace_center=False):
    path = shared_skill_mutation_path(path)
    detach_center_symlink_dependents(path)
    if replace_center:
        detach_center_symlink_dependents(center_source)
    path = shared_skill_mutation_path(path)
    if paths_overlap(center_source, path):
        raise ValueError("Center and shared Skill paths overlap after safe detachment")
    return path


def remove_shared_skill_path(path):
    path = shared_skill_mutation_path(path)
    detach_center_symlink_dependents(path)
    path = shared_skill_mutation_path(path)
    if center_symlink_dependents(path):
        raise ValueError("Dependent center Skill links still reference the shared source")
    remove_path(path)


def copy_or_link(source, target, mode):
    target.parent.mkdir(parents=True, exist_ok=True)
    if mode == "link":
        target.symlink_to(source, target_is_directory=True)
        return "link"
    shutil.copytree(source, target, symlinks=True)
    return "copy"


def pack_applied_agent_ids(pack_id):
    agents = set(STATE["packs"].get(pack_id, {}).get("appliedAgents", []))
    agents.update(
        target_id.partition("::")[0]
        for target_id in STATE["packTargetClaims"].get(pack_id, {})
    )
    return sorted(agents)


def applied_pack_ids(agent_id):
    pack_ids = dict.fromkeys(list(STATE["packs"]) + list(STATE["packTargetClaims"]))
    return [pack_id for pack_id in pack_ids if agent_id in pack_applied_agent_ids(pack_id)]


def add_pack_target_claims(pack_id, target_ids):
    claims = dict.fromkeys(target_ids, True)
    if claims:
        STATE["packTargetClaims"].setdefault(pack_id, {}).update(claims)


def filter_pack_target_claims(pack_id, keep):
    claims = STATE["packTargetClaims"].get(pack_id, {})
    remaining = {target_id: True for target_id in claims if keep(target_id)}
    if remaining:
        STATE["packTargetClaims"][pack_id] = remaining
    else:
        STATE["packTargetClaims"].pop(pack_id, None)
    return remaining != claims


def pack_claims(agent_id, skill_id):
    claims = []
    stamp = now()
    target_id = agent_id + "::" + skill_id
    for pack_id in dict.fromkeys(list(STATE["packs"]) + list(STATE["packTargetClaims"])):
        pack = STATE["packs"].get(pack_id, {})
        explicit = target_id in STATE["packTargetClaims"].get(pack_id, {})
        legacy = agent_id in pack.get("appliedAgents", []) and skill_id in pack.get("skillIds", [])
        if explicit or legacy:
            claims.append({
                "id": "pack::" + pack_id + "::" + agent_id + "::" + skill_id,
                "claimType": "pack",
                "packId": pack_id,
                "packName": DEFAULT_PACK_NAME if pack_id == "default" else pack.get("name", pack_id),
                "createdAt": pack.get("createdAt", stamp),
            })
    if not claims:
        claims.append({
            "id": "direct::" + agent_id + "::" + skill_id,
            "claimType": "direct",
            "packId": None,
            "packName": None,
            "createdAt": stamp,
        })
    return claims


def inventory():
    center = center_path()
    skills = {}
    for path in skill_directories(center):
        skill_id = path.name
        meta = frontmatter(path / "SKILL.md", skill_id)
        source = STATE["sources"].get(skill_id, {})
        skills[skill_id] = {
            "id": skill_id,
            "name": meta["name"],
            "description": meta["description"],
            "skillType": "skill",
            "sourceType": source.get("sourceType", "remote_center"),
            "sourceUri": source.get("sourceUri"),
            "centerPath": str(path),
            "currentHash": hash_dir(path),
            "status": "ok",
            "installedAgents": [],
        }
    unmanaged = []
    agents = []
    targets = {}
    for spec in management_agent_specs():
        root = agent_skills_path(spec["id"])
        installed = agent_installed(spec, agent_binary(spec))
        managed_count = 0
        unmanaged_count = 0
        for path in skill_directories(root):
            skill_id = path.name
            if skill_id in skills:
                actual_mode = "link" if path.is_symlink() else "copy"
                source_hash = skills[skill_id]["currentHash"]
                current_hash = hash_dir(path.resolve() if path.is_symlink() else path)
                status = "ok" if source_hash == current_hash else "copyDiverged"
                skills[skill_id]["installedAgents"].append({
                    "agentId": spec["id"],
                    "displayName": spec["displayName"],
                    "iconKey": spec["iconKey"],
                    "mode": actual_mode,
                    "status": status,
                })
                target = {
                    "id": spec["id"] + "::" + skill_id,
                    "skillId": skill_id,
                    "agentId": spec["id"],
                    "targetPath": str(path),
                    "resolvedTargetPath": str(path.resolve(strict=False)),
                    "installMode": actual_mode,
                    "actualMode": actual_mode,
                    "sourceHash": source_hash,
                    "currentHash": current_hash,
                    "status": status,
                    "createdAt": now(),
                    "updatedAt": now(),
                    "claims": pack_claims(spec["id"], skill_id),
                }
                targets[target["id"]] = target
                managed_count += 1
            else:
                unmanaged_id = spec["id"] + "::" + skill_id
                unmanaged.append({
                    "id": unmanaged_id,
                    "itemType": "skill",
                    "agentId": spec["id"],
                    "path": str(path),
                    "inferredSkillId": skill_id,
                    "hash": hash_dir(path),
                    "reason": "Agent Skill is not managed by the remote center library",
                    "readOnly": False,
                })
                unmanaged_count += 1
        agents.append({
            "id": spec["id"],
            "displayName": spec["displayName"],
            "iconKey": spec["iconKey"],
            "enabled": True,
            "skillsDir": str(root),
            "version": None,
            "latestVersion": None,
            "installed": bool(installed),
            "managedSkillCount": managed_count,
            "unmanagedSkillCount": unmanaged_count,
            "readOnlySkillCount": 0,
        })
    shared_root = shared_skills_path()
    for path in skill_directories(shared_root):
        skill_id = path.name
        if skill_id in skills:
            actual_mode = "link" if path.is_symlink() else "copy"
            source_hash = skills[skill_id]["currentHash"]
            current_hash = hash_dir(path.resolve() if path.is_symlink() else path)
            status = "ok" if source_hash == current_hash else "copyDiverged"
            skills[skill_id]["installedAgents"].append({
                "agentId": SHARED_SKILLS_AGENT_ID,
                "displayName": ".agents",
                "iconKey": SHARED_SKILLS_AGENT_ID,
                "mode": actual_mode,
                "status": status,
            })
            target = {
                "id": SHARED_SKILLS_AGENT_ID + "::" + skill_id,
                "skillId": skill_id,
                "agentId": SHARED_SKILLS_AGENT_ID,
                "targetPath": str(path),
                "resolvedTargetPath": str(path.resolve(strict=False)),
                "installMode": actual_mode,
                "actualMode": actual_mode,
                "sourceHash": source_hash,
                "currentHash": current_hash,
                "status": status,
                "createdAt": now(),
                "updatedAt": now(),
                "claims": pack_claims(SHARED_SKILLS_AGENT_ID, skill_id),
            }
            targets[target["id"]] = target
        else:
            unmanaged_id = SHARED_SKILLS_AGENT_ID + "::" + skill_id
            unmanaged.append({
                "id": unmanaged_id,
                "itemType": "skill",
                "agentId": SHARED_SKILLS_AGENT_ID,
                "path": str(path),
                "inferredSkillId": skill_id,
                "hash": hash_dir(path),
                "reason": "shared_agents_directory",
                "readOnly": False,
            })
    return skills, agents, unmanaged, targets


def shared_skill_projection(agent_id, unmanaged, targets):
    if agent_id not in SHARED_SKILL_CONSUMERS:
        return [], []
    managed_by_path = {}
    for target in sorted(
        (item for item in targets.values()
         if item["agentId"] == SHARED_SKILLS_AGENT_ID),
        key=lambda item: (item["targetPath"], item["id"]),
    ):
        path = str(pathlib.Path(target["targetPath"]).resolve(strict=False))
        managed_by_path.setdefault(path, target)
    unmanaged_by_path = {}
    for item in sorted(
        (item for item in unmanaged
         if item["agentId"] == SHARED_SKILLS_AGENT_ID),
        key=lambda item: (item["path"], item["id"]),
    ):
        path = str(pathlib.Path(item["path"]).resolve(strict=False))
        if path not in managed_by_path:
            unmanaged_by_path.setdefault(path, item)
    managed_skills = sorted(
        managed_by_path.values(),
        key=lambda item: (item["skillId"].lower(), item["targetPath"]),
    )
    unmanaged_skills = sorted(
        unmanaged_by_path.values(),
        key=lambda item: (
            str(item.get("inferredSkillId") or pathlib.Path(item["path"]).name).lower(),
            item["path"],
        ),
    )
    return managed_skills, unmanaged_skills


def pack_detail(pack_id, skills=None, agents=None):
    if skills is None or agents is None:
        skills, agents, _, _ = inventory()
    if pack_id == "default":
        data = {
            "id": "default",
            "name": DEFAULT_PACK_NAME,
            "description": "中心库全部 Skills。应用时按当前中心库全量分发。",
            "tags": [],
            "skillIds": list(skills),
            "revision": 1,
            "createdAt": now(),
            "updatedAt": now(),
        }
    else:
        data = STATE["packs"].get(pack_id)
        if not data:
            raise ValueError("Skill Pack not found: " + pack_id)
    members = []
    for index, skill_id in enumerate(data.get("skillIds", [])):
        skill = skills.get(skill_id)
        members.append({
            "skillId": skill_id,
            "skillName": skill["name"] if skill else skill_id,
            "required": True,
            "sortOrder": index,
            "missing": skill is None,
        })
    applied = []
    for agent_id in pack_applied_agent_ids(pack_id):
        agent = next((item for item in agents if item["id"] == agent_id), None)
        applied.append({
            "packId": data["id"],
            "packName": data["name"],
            "memberCount": len(members),
            "agentId": agent_id,
            "displayName": agent["displayName"] if agent else agent_id,
            "iconKey": agent["iconKey"] if agent else agent_id,
            "packRevision": data.get("revision", 1),
            "syncedRevision": data.get("revision", 1),
            "syncStatus": "synced",
            "syncError": None,
        })
    return {
        "id": data["id"],
        "name": data["name"],
        "description": data.get("description", ""),
        "tags": data.get("tags", []),
        "members": members,
        "appliedAgents": applied,
        "revision": data.get("revision", 1),
        "syncStatus": "synced",
        "pendingSyncCount": 0,
        "failedSyncCount": 0,
        "createdAt": data.get("createdAt", now()),
        "updatedAt": data.get("updatedAt", now()),
    }


def pack_summary(detail):
    return {
        "id": detail["id"],
        "name": detail["name"],
        "description": detail["description"],
        "tags": detail["tags"],
        "memberCount": len(detail["members"]),
        "appliedAgentCount": len(detail["appliedAgents"]),
        "healthy": not any(item["missing"] for item in detail["members"]),
        "revision": detail["revision"],
        "syncStatus": detail["syncStatus"],
        "pendingSyncCount": detail["pendingSyncCount"],
        "failedSyncCount": detail["failedSyncCount"],
    }


def overview_from_inventory(skills, agents, unmanaged, targets):
    details = [pack_detail("default", skills, agents)]
    details.extend(
        pack_detail(pack_id, skills, agents)
        for pack_id in sorted(STATE["packs"])
        if pack_id != "default"
    )
    issues = diagnosis(skills, agents, unmanaged, targets)
    return {
        "metrics": {
            "centerSkillCount": len(skills),
            "targetCount": len(targets),
            "unmanagedCount": len(unmanaged),
            "issueCount": len(issues),
        },
        "skills": sorted(skills.values(), key=lambda item: item["name"].lower()),
        "agents": agents,
        "packs": [pack_summary(item) for item in details],
        "issues": issues,
        "settings": STATE["settings"],
    }


def overview():
    return overview_from_inventory(*inventory())


def file_tree(path):
    path = safe_home_path(path, allow_missing=False)
    if path.is_file():
        return {"name": path.name, "nodeType": "file", "path": str(path), "children": None}
    children = []
    for child in sorted(path.iterdir(), key=lambda item: (not item.is_dir(), item.name.lower())):
        if child.name == ".git":
            continue
        children.append(file_tree(child))
    return {"name": path.name, "nodeType": "dir", "path": str(path), "children": children}


def skill_detail(skill_id):
    skill_id = safe_id(skill_id)
    skills, _, _, targets = inventory()
    skill = skills.get(skill_id)
    if not skill:
        raise ValueError("Skill not found: " + skill_id)
    path = pathlib.Path(skill["centerPath"])
    source = STATE["sources"].get(skill_id)
    return {
        **skill,
        "centerResolvedPath": str(path.resolve(strict=False)),
        "frontmatter": frontmatter(path / "SKILL.md", skill_id),
        "files": file_tree(path),
        "targets": [item for item in targets.values() if item["skillId"] == skill_id],
        "source": {
            "sourceType": source.get("sourceType", "remote_center"),
            "sourceUri": source.get("sourceUri"),
            "sourceRef": source.get("sourceRef"),
            "importedFromAgent": source.get("importedFromAgent"),
            "importedFromPath": source.get("importedFromPath"),
            "installedVia": source.get("installedVia", "remote"),
            "createdAt": source.get("createdAt", now()),
            "updatedAt": source.get("updatedAt", now()),
        } if source else None,
    }


def affected_target(target, agents):
    agent = next((item for item in agents if item["id"] == target["agentId"]), None)
    return {
        "targetId": target["id"],
        "agentId": target["agentId"],
        "displayName": agent["displayName"] if agent else target["agentId"],
        "targetPath": target["targetPath"],
        "mode": target["actualMode"],
        "claimCount": len(target["claims"]),
    }


def targets_claimed_by_pack(pack_id, targets):
    return (
        target for target in targets.values()
        if any(claim["packId"] == pack_id for claim in target["claims"])
    )


def preview_delete(skill_ids):
    skills, agents, _, targets = inventory()
    normalized = [safe_id(item) for item in skill_ids]
    affected = [
        affected_target(target, agents)
        for target in targets.values()
        if target["skillId"] in normalized
    ]
    return {
        "skillId": normalized[0] if normalized else "",
        "skillIds": normalized,
        "affectedTargets": affected,
        "removable": all(item in skills for item in normalized),
        "warnings": [],
    }


def distribution_preview(skill_ids, target_agents, requested_mode):
    skills, _, _, _ = inventory()
    mode = requested_mode if requested_mode in ("link", "copy") else "copy"
    changes = []
    blockers = []
    for skill_id in skill_ids:
        skill_id = safe_id(skill_id)
        if skill_id not in skills:
            raise ValueError("Skill not found: " + skill_id)
        source = pathlib.Path(skills[skill_id]["centerPath"])
        for agent_id in target_agents:
            target = agent_skills_path(agent_id) / skill_id
            if not target.exists() and not target.is_symlink():
                changes.append({
                    "skillId": skill_id,
                    "agentId": agent_id,
                    "action": "create",
                    "actualMode": mode,
                    "reason": None,
                    "targetPath": str(target),
                })
                continue
            same = False
            if target.is_symlink():
                same = target.resolve(strict=False) == source.resolve(strict=False)
            elif target.is_dir():
                same = hash_dir(target) == skills[skill_id]["currentHash"]
            if same:
                changes.append({
                    "skillId": skill_id,
                    "agentId": agent_id,
                    "action": "reuse",
                    "actualMode": "link" if target.is_symlink() else "copy",
                    "reason": None,
                    "targetPath": str(target),
                })
            else:
                blockers.append({
                    "skillId": skill_id,
                    "agentId": agent_id,
                    "reason": "The target already contains different content",
                    "existingPath": str(target),
                    "existingPathKind": "symlink" if target.is_symlink() else "directory",
                    "resolvedExistingPath": str(target.resolve(strict=False)),
                })
                changes.append({
                    "skillId": skill_id,
                    "agentId": agent_id,
                    "action": "blocked",
                    "actualMode": mode,
                    "reason": "The target already contains different content",
                    "targetPath": str(target),
                })
    return {
        "skillIds": skill_ids,
        "targetAgents": target_agents,
        "requestedMode": mode,
        "changes": changes,
        "blockers": blockers,
        "blockerDecisions": [],
    }


def text_or_none(path):
    if not path.is_file() or path.stat().st_size > 256 * 1024:
        return None
    try:
        return path.read_text()
    except Exception:
        return None


def copy_diff_files(center, target):
    center_files = {
        str(item.relative_to(center)): item
        for item in center.rglob("*")
        if item.is_file()
    }
    target_files = {
        str(item.relative_to(target)): item
        for item in target.rglob("*")
        if item.is_file()
    }
    result = []
    for relative in sorted(set(center_files) | set(target_files)):
        center_file = center_files.get(relative)
        target_file = target_files.get(relative)
        if center_file and target_file and hash_file(center_file) == hash_file(target_file):
            continue
        result.append({
            "path": relative,
            "changeType": (
                "copy_added" if center_file is None
                else "copy_removed" if target_file is None
                else "modified"
            ),
            "centerContent": text_or_none(center_file) if center_file else None,
            "copyContent": text_or_none(target_file) if target_file else None,
        })
    return result


def execute_distribution(preview):
    decisions = {
        (item["skillId"], item["agentId"]): item["action"]
        for item in preview.get("blockerDecisions", [])
    }
    mode = preview.get("requestedMode", "copy")
    skills, _, _, _ = inventory()
    result = distribution_preview(
        preview.get("skillIds", []),
        preview.get("targetAgents", []),
        mode,
    )
    completed = []
    for change in result["changes"]:
        skill_id = change["skillId"]
        agent_id = change["agentId"]
        source = pathlib.Path(skills[skill_id]["centerPath"])
        target = agent_skills_path(agent_id) / skill_id
        action = change["action"]
        decision = decisions.get((skill_id, agent_id))
        if action == "blocked":
            if decision == "skip" or decision is None:
                completed.append({**change, "action": "skip"})
                continue
            if decision == "agent_over_center":
                remove_path(source)
                shutil.copytree(target, source, symlinks=True)
                completed.append({**change, "action": "reuse"})
                continue
            if decision != "overwrite":
                completed.append({**change, "action": "skip"})
                continue
            remove_path(target)
        if action != "reuse":
            actual_mode = copy_or_link(source, target, mode)
            completed.append({**change, "action": "create", "actualMode": actual_mode})
        else:
            completed.append(change)
    return {
        **result,
        "changes": completed,
        "blockerDecisions": preview.get("blockerDecisions", []),
    }


def diagnosis(skills=None, agents=None, unmanaged=None, targets=None):
    if skills is None:
        skills, agents, unmanaged, targets = inventory()
    issues = []
    for item in unmanaged:
        issues.append({
            "id": "unmanaged::" + item["id"],
            "issueType": "unmanaged_skill",
            "severity": "warning",
            "fixKind": "confirm",
            "title": "Unmanaged Agent Skill",
            "detail": item["path"],
            "entityType": "skill",
            "entityId": item["id"],
            "actions": [{"id": "adopt", "label": "Adopt", "destructive": False}],
        })
    for target in targets.values():
        if target["status"] != "ok":
            issues.append({
                "id": "target::" + target["id"],
                "issueType": "copy_diverged",
                "severity": "warning",
                "fixKind": "confirm",
                "title": "Distributed Skill differs from center",
                "detail": target["targetPath"],
                "entityType": "target",
                "entityId": target["id"],
                "actions": [{"id": "sync", "label": "Sync", "destructive": True}],
            })
    return issues


def unmanaged_inventory_from_inventory(skills, agents, unmanaged, targets):
    by_agent = {agent["id"]: [] for agent in agents}
    for item in unmanaged:
        skill_id = item["inferredSkillId"] or pathlib.Path(item["path"]).name
        by_agent.setdefault(item["agentId"], []).append({
            "id": item["id"],
            "agentId": item["agentId"],
            "skillId": skill_id,
            "name": frontmatter(pathlib.Path(item["path"]) / "SKILL.md", skill_id)["name"],
            "path": item["path"],
            "managed": False,
            "readOnly": item.get("readOnly", False),
            "canImport": not item.get("readOnly", False),
            "status": "unmanaged",
            "statusLabel": "未管理",
            "reason": item["reason"],
            "targetId": None,
            "actualMode": None,
            "hash": item["hash"],
        })
    for target in targets.values():
        skill = skills[target["skillId"]]
        by_agent.setdefault(target["agentId"], []).append({
            "id": target["id"],
            "agentId": target["agentId"],
            "skillId": target["skillId"],
            "name": skill["name"],
            "path": target["targetPath"],
            "managed": True,
            "readOnly": False,
            "canImport": False,
            "status": target["status"],
            "statusLabel": "正常" if target["status"] == "ok" else "有变更",
            "reason": None,
            "targetId": target["id"],
            "actualMode": target["actualMode"],
            "hash": target["currentHash"],
        })
    result = []
    for agent in agents:
        items = by_agent.get(agent["id"], [])
        result.append({
            "agentId": agent["id"],
            "displayName": agent["displayName"],
            "iconKey": agent["iconKey"],
            "skillsDir": agent["skillsDir"],
            "installed": agent["installed"],
            "managedCount": sum(1 for item in items if item["managed"]),
            "unmanagedCount": sum(1 for item in items if not item["managed"]),
            "readOnlyCount": sum(1 for item in items if item.get("readOnly")),
            "importableCount": sum(1 for item in items if item["canImport"]),
            "items": sorted(items, key=lambda item: item["name"].lower()),
        })
    shared_items = by_agent.get(SHARED_SKILLS_AGENT_ID, [])
    result.append({
        "agentId": SHARED_SKILLS_AGENT_ID,
        "displayName": ".agents",
        "iconKey": SHARED_SKILLS_AGENT_ID,
        "skillsDir": str(shared_skills_path()),
        "installed": shared_skills_path().is_dir(),
        "managedCount": sum(1 for item in shared_items if item["managed"]),
        "unmanagedCount": sum(1 for item in shared_items if not item["managed"]),
        "readOnlyCount": sum(1 for item in shared_items if item.get("readOnly")),
        "importableCount": sum(1 for item in shared_items if item["canImport"]),
        "items": sorted(shared_items, key=lambda item: item["name"].lower()),
    })
    return result


def unmanaged_inventory():
    return unmanaged_inventory_from_inventory(*inventory())


def find_unmanaged(unmanaged_id):
    _, _, unmanaged, _ = inventory()
    item = next((item for item in unmanaged if item["id"] == unmanaged_id), None)
    if not item:
        raise ValueError("Unmanaged Skill not found: " + unmanaged_id)
    return item


def takeover_center_agent_skill(agent_id, unmanaged_id, skills, targets):
    # Remote inventory represents same-name copies as Skill targets.
    target = targets.get(unmanaged_id)
    if not target:
        raise ValueError("The requested Agent Skill is not present in the center library")
    if target["agentId"] != agent_id or agent_id == SHARED_SKILLS_AGENT_ID:
        raise ValueError("The requested Skill does not belong to this Agent's own directory")
    skill_id = target["skillId"]
    center = pathlib.Path(skills[skill_id]["centerPath"])
    agent_copy = pathlib.Path(target["targetPath"])
    if not (center / "SKILL.md").is_file() or not (agent_copy / "SKILL.md").is_file():
        raise ValueError("The center or Agent Skill is unavailable on disk")
    if agent_copy.is_symlink():
        link_target = normalized_lexical_path(agent_copy.parent / readlink(agent_copy))
        if link_target == normalized_lexical_path(center):
            return skill_id
        raise ValueError("The Agent Skill already links to a different location")
    atomic_replace_skill_directory(center, agent_copy, link=True)
    return skill_id


def remove_unmanaged_item(item, requested_agent_id):
    owner = item.get("agentId")
    if owner != requested_agent_id:
        raise ValueError("Unmanaged Skill does not belong to this Agent")
    if owner == SHARED_SKILLS_AGENT_ID:
        remove_shared_skill_path(item["path"])
        return
    remove_path(pathlib.Path(item["path"]))


def remove_target_distribution(target):
    if target.get("agentId") == SHARED_SKILLS_AGENT_ID:
        remove_shared_skill_path(target["targetPath"])
    else:
        remove_path(pathlib.Path(target["targetPath"]))
    changed = False
    for pack_id in list(STATE["packTargetClaims"]):
        if filter_pack_target_claims(pack_id, lambda target_id: target_id != target["id"]):
            changed = True
    return changed


def adopt_preview(agent_id, unmanaged_id):
    item = find_unmanaged(unmanaged_id)
    if item["agentId"] != agent_id:
        raise ValueError("Unmanaged Skill does not belong to this Agent")
    if agent_id == SHARED_SKILLS_AGENT_ID:
        shared_skill_mutation_path(item["path"])
    skill_id = safe_id(item["inferredSkillId"])
    target = center_path() / skill_id
    same_id = target.exists() or target.is_symlink()
    if agent_id == SHARED_SKILLS_AGENT_ID:
        if not same_id:
            options = [{
                "value": "import_cleanup",
                "label": "Import to center and remove the shared .agents copy",
                "destructive": True,
            }]
        else:
            options = []
            if independent_center_skill_hash(target):
                options.append({
                    "value": "center_over_agent",
                    "label": "Use center skill and remove the shared .agents copy",
                    "destructive": True,
                })
            options.extend([
                {
                    "value": "overwrite_center",
                    "label": "Overwrite center skill and remove the shared .agents copy",
                    "destructive": True,
                },
                {
                    "value": "rename",
                    "label": "Import under a new id and remove the shared .agents copy",
                    "destructive": True,
                },
                {
                    "value": "skip",
                    "label": "Keep as unmanaged",
                    "destructive": False,
                },
            ])
    else:
        options = [
            {
                "value": "import_link",
                "label": "导入中心库并链接",
                "destructive": False,
            },
            {
                "value": "import_cleanup",
                "label": "导入中心库并整理副本",
                "destructive": True,
            },
            {
                "value": "center_over_agent",
                "label": "使用中心库覆盖 Agent",
                "destructive": True,
            },
            {
                "value": "overwrite_center",
                "label": "使用 Agent 覆盖中心库",
                "destructive": True,
            },
            {
                "value": "rename",
                "label": "重命名后导入",
                "destructive": False,
            },
            {
                "value": "skip",
                "label": "跳过",
                "destructive": False,
            },
        ]
    return {
        "agentId": agent_id,
        "unmanagedId": unmanaged_id,
        "skillPath": item["path"],
        "inferredSkillId": skill_id,
        "hash": item["hash"],
        "centerHasSameId": same_id,
        "canQuickAdopt": not same_id,
        "options": options,
    }


def execute_adopt(agent_id, unmanaged_id, option, renamed_id=None):
    preview = adopt_preview(agent_id, unmanaged_id)
    if agent_id == SHARED_SKILLS_AGENT_ID:
        allowed_options = [item["value"] for item in preview["options"]]
        if option not in allowed_options:
            raise ValueError("Adopt option is not allowed for a shared .agents Skill")
    if option == "skip":
        return ""
    source = pathlib.Path(preview["skillPath"])
    if agent_id == SHARED_SKILLS_AGENT_ID:
        source = shared_skill_mutation_path(source)
    skill_id = safe_id(renamed_id if option == "rename" else preview["inferredSkillId"])
    target = center_path() / skill_id
    if agent_id == SHARED_SKILLS_AGENT_ID:
        detach_center_symlink_dependents(source)
        source = shared_skill_mutation_path(source)
        target_present = target.exists() or target.is_symlink()
        if target_present and option == "center_over_agent":
            if not independent_center_skill_hash(target):
                raise ValueError("Center Skill is not an independent readable Skill directory")
            remove_shared_skill_path(source)
            return skill_id
        if target_present and option == "overwrite_center":
            detach_center_symlink_dependents(target)
        elif target_present:
            raise ValueError("Center Skill already exists: " + skill_id)
        atomic_replace_skill_directory(
            source,
            target,
            after_replace=lambda: remove_shared_skill_path(source),
        )
        STATE["sources"][skill_id] = {
            "sourceType": "agent_import",
            "sourceUri": None,
            "sourceRef": None,
            "importedFromAgent": agent_id,
            "importedFromPath": str(source),
            "installedVia": "remote_adopt",
            "createdAt": now(),
            "updatedAt": now(),
        }
        save_state()
        return skill_id
    if target.exists() and option == "center_over_agent":
        remove_path(source)
        copy_or_link(target, source, STATE["settings"]["defaultDistributeMode"])
        return skill_id
    if target.exists() and option == "overwrite_center":
        remove_path(target)
    elif target.exists() and option not in ("overwrite_center",):
        raise ValueError("Center Skill already exists: " + skill_id)
    shutil.copytree(source, target, symlinks=True)
    STATE["sources"][skill_id] = {
        "sourceType": "agent_import",
        "sourceUri": None,
        "sourceRef": None,
        "importedFromAgent": agent_id,
        "importedFromPath": str(source),
        "installedVia": "remote_adopt",
        "createdAt": now(),
        "updatedAt": now(),
    }
    if option in ("import_link", "import_cleanup"):
        remove_path(source)
        copy_or_link(target, source, "link" if option == "import_link" else "copy")
    save_state()
    return skill_id


def normalize_repository(source):
    source = str(source or "").strip().rstrip("/")
    if source.startswith("skillssh:"):
        spec = source[len("skillssh:"):].strip("/")
        parts = [item for item in spec.split("/") if item]
        if len(parts) < 3:
            raise ValueError("skills.sh source must be owner/repository/skill-id")
        return "https://github.com/" + parts[0] + "/" + parts[1] + ".git", "/".join(parts[2:]), None
    if source.startswith("https://skills.sh/"):
        spec = source[len("https://skills.sh/"):].strip("/")
        parts = [item for item in spec.split("/") if item]
        if len(parts) < 3:
            raise ValueError("skills.sh source must be owner/repository/skill-id")
        return "https://github.com/" + parts[0] + "/" + parts[1] + ".git", "/".join(parts[2:]), None
    if source.startswith("https://raw.githubusercontent.com/"):
        parts = [
            item for item in urllib.parse.urlparse(source).path.strip("/").split("/")
            if item
        ]
        if len(parts) >= 4:
            requested = parts[3:]
            if requested and requested[-1] == "SKILL.md":
                requested = requested[:-1]
            return (
                "https://github.com/" + parts[0] + "/" + parts[1] + ".git",
                "/".join(urllib.parse.unquote(item) for item in requested) or None,
                urllib.parse.unquote(parts[2]),
            )
    if source.startswith("https://github.com/"):
        parsed = urllib.parse.urlparse(source)
        parts = [item for item in parsed.path.strip("/").split("/") if item]
        if len(parts) >= 4 and parts[2] in ("tree", "blob"):
            requested = parts[4:]
            if requested and requested[-1] == "SKILL.md":
                requested = requested[:-1]
            return (
                "https://github.com/" + parts[0] + "/" + parts[1] + ".git",
                "/".join(urllib.parse.unquote(item) for item in requested) or None,
                urllib.parse.unquote(parts[3]),
            )
    if source.startswith(("https://github.com/", "https://gitlab.com/", "git@github.com:", "git@gitlab.com:")):
        return source, None, None
    return None, None, None


def resolve_repository_source(source):
    repository, requested, reference = normalize_repository(source)
    if (not requested or not reference or "/" in reference or reference == "HEAD"
            or re.fullmatch(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})", reference)):
        return repository, requested, reference

    pieces = [reference, *requested.split("/")]
    names = [
        "/".join(pieces[:length])
        for length in range(1, len(pieces) + 1)
    ]
    if not all(re.fullmatch(r"[A-Za-z0-9._/-]+", name) for name in names):
        return repository, requested, reference
    patterns = [f"refs/{namespace}/{name}" for name in names for namespace in ("heads", "tags")]
    output = run_repository_git(HOME, "ls-remote", "--refs", "--", repository, *patterns)
    advertised = {line.partition("\t")[2] for line in output.splitlines()}
    for index in range(len(names) - 1, -1, -1):
        name = names[index]
        if "refs/heads/" + name in advertised or "refs/tags/" + name in advertised:
            return repository, "/".join(pieces[index + 1:]) or None, name
    return repository, requested, reference


def source_candidates(input_value):
    source_path = str(input_value.get("sourcePath") or "")
    repository, requested, reference = resolve_repository_source(
        input_value.get("sourceUri") or source_path
    )
    if repository:
        root, temporary = clone_repository(repository, reference)
        try:
            candidates = repository_skill_directories(root, requested, reference is not None)
        except Exception:
            temporary.cleanup()
            raise
        return root, candidates, temporary, repository, reference

    temporary = None
    root = safe_home_path(source_path, allow_missing=False)
    if root.is_file() and root.suffix.lower() == ".zip":
        temporary = tempfile.TemporaryDirectory(prefix="agentbro-remote-archive-")
        extracted = pathlib.Path(temporary.name) / "extracted"
        extracted.mkdir(parents=True, exist_ok=True)
        shutil.unpack_archive(str(root), str(extracted), "zip")
        root = extracted
    candidates = []
    if root.is_dir() and (root / "SKILL.md").is_file():
        candidates = [root]
    elif root.is_dir():
        candidates = [
            path.parent for path in root.rglob("SKILL.md")
            if len(path.relative_to(root).parts) <= 7
        ]
    return root, candidates, temporary, None, None


def preview_add(input_value):
    _root, candidates, temporary, _repository, _reference = source_candidates(input_value)
    try:
        center = center_path()
        result = []
        blockers = []
        unchanged = 0
        for source in candidates:
            skill_id = safe_id(source.name)
            meta = frontmatter(source / "SKILL.md", skill_id)
            existing = center / skill_id
            action = "create"
            reason = None
            existing_type = None
            if existing.exists():
                if hash_dir(existing) == hash_dir(source):
                    unchanged += 1
                    continue
                action = "blocked_same_name_diff_source"
                reason = "Center library already contains a different Skill with this ID"
                existing_type = STATE["sources"].get(skill_id, {}).get("sourceType")
            item = {
                "skillId": skill_id,
                "proposedSkillId": skill_id,
                "name": meta["name"],
                "description": meta["description"],
                "sourceDir": str(source),
                "hash": hash_dir(source),
                "action": action,
                "existingSourceType": existing_type,
                "reason": reason,
            }
            result.append(item)
            if action.startswith("blocked"):
                blockers.append(item)
        return {
            "candidates": result,
            "blockers": blockers,
            "unchangedCount": unchanged,
            "centerPath": str(center),
        }
    finally:
        if temporary:
            temporary.cleanup()


def execute_add(input_value, decisions):
    root, candidates, temporary, repository, reference = source_candidates(input_value)
    try:
        decision_map = {item["skillId"]: item for item in decisions}
        revision = run_repository_git(root, "rev-parse", "HEAD") if repository else None
        center = center_path()
        installed = []
        updated = []
        skipped = []
        for source in candidates:
            original_id = safe_id(source.name)
            decision = decision_map.get(original_id, {})
            resolution = decision.get("resolution", "create")
            skill_id = safe_id(decision.get("proposedSkillId") or original_id)
            target = center / skill_id
            if resolution == "skip":
                skipped.append(original_id)
                continue
            if target.exists():
                if resolution != "update":
                    skipped.append(original_id)
                    continue
                remove_path(target)
                updated.append(skill_id)
            should_link = (
                input_value.get("sourceType") == "local_folder"
                and input_value.get("importMode") == "link"
                and source.is_dir()
            )
            if should_link:
                target.symlink_to(source, target_is_directory=True)
            else:
                shutil.copytree(source, target, symlinks=True)
            if skill_id not in updated:
                installed.append(skill_id)
            STATE["sources"][skill_id] = {
                "sourceType": input_value.get("sourceType", "remote_import"),
                "sourceUri": (repository_skill_source(repository, reference, source.relative_to(root).as_posix())
                              if repository else input_value.get("sourceUri")),
                "sourceRef": revision,
                "importedFromAgent": input_value.get("importedFromAgent"),
                "importedFromPath": input_value.get("importedFromPath"),
                "installedVia": "remote",
                "createdAt": now(),
                "updatedAt": now(),
            }
        save_state()
        return {"skillIds": installed, "updated": updated, "skipped": skipped}
    finally:
        if temporary:
            temporary.cleanup()


def upsert_pack(pack):
    pack_id = "pack-" + uuid.uuid4().hex if pack["id"] == "" else safe_id(pack["id"])
    if pack_id == "default":
        raise ValueError("The default Skill Pack cannot be edited")
    existing = STATE["packs"].get(pack_id, {})
    skill_ids = [safe_id(item) for item in pack.get("skillIds", [])]
    filter_pack_target_claims(pack_id, lambda target_id: target_id.partition("::")[2] in skill_ids)
    stamp = now()
    STATE["packs"][pack_id] = {
        "id": pack_id,
        "name": str(pack.get("name") or pack_id),
        "description": str(pack.get("description") or ""),
        "tags": [str(item) for item in pack.get("tags", [])],
        "skillIds": skill_ids,
        "appliedAgents": existing.get("appliedAgents", []),
        "revision": int(existing.get("revision", 0)) + 1,
        "createdAt": existing.get("createdAt", stamp),
        "updatedAt": stamp,
    }
    save_state()
    return pack_detail(pack_id)


def apply_pack(pack_id, target_agents, requested_mode, decisions=None):
    detail = pack_detail(pack_id)
    skill_ids = [item["skillId"] for item in detail["members"] if not item["missing"]]
    preview = distribution_preview(skill_ids, target_agents, requested_mode)
    preview["blockerDecisions"] = decisions or []
    result = execute_distribution(preview)
    if pack_id != "default":
        pack = STATE["packs"][pack_id]
        pack["appliedAgents"] = sorted(set(pack.get("appliedAgents", []) + target_agents))
        pack["updatedAt"] = now()
        save_state()
    return result


def agent_detail_from_inventory(
    agent_id,
    skills,
    agents,
    unmanaged,
    targets,
    available_packs=None,
):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + agent_id)
    agent = next(item for item in agents if item["id"] == agent_id)
    inherited_managed, inherited_unmanaged = shared_skill_projection(
        agent_id,
        unmanaged,
        targets,
    )
    applied = []
    for pack_id in applied_pack_ids(agent_id):
        detail = pack_detail(pack_id, skills, agents)
        applied.extend(item for item in detail["appliedAgents"] if item["agentId"] == agent_id)
    return {
        "id": agent_id,
        "displayName": spec["displayName"],
        "iconKey": spec["iconKey"],
        "version": agent["version"],
        "latestVersion": agent["latestVersion"],
        "skillsDir": agent["skillsDir"],
        "configPath": str(agent_config_path(agent_id)),
        "mcpConfigPath": str(agent_mcp_path(agent_id)),
        "pluginDir": str(agent_plugin_path(agent_id)),
        "agentDir": str(spec.get("configDir") or agent_skills_path(agent_id).parent),
        "skills": [item for item in targets.values() if item["agentId"] == agent_id],
        "inheritsSharedSkills": agent_id in SHARED_SKILL_CONSUMERS,
        "inheritedManagedSkills": inherited_managed,
        "inheritedUnmanagedSkills": inherited_unmanaged,
        "appliedPacks": applied,
        "availablePacks": available_packs if available_packs is not None else overview_from_inventory(
            skills,
            agents,
            unmanaged,
            targets,
        )["packs"],
        "mcpServers": [],
        "plugins": plugin_inventory(agent_id)["plugins"],
        "health": [],
    }


def agent_detail(agent_id):
    return agent_detail_from_inventory(agent_id, *inventory())


def agent_skill_view_snapshot(agent_id):
    snapshot = inventory()
    overview_value = overview_from_inventory(*snapshot)
    return {
        "agentDetail": agent_detail_from_inventory(
            agent_id,
            *snapshot,
            available_packs=overview_value["packs"],
        ),
        "overview": overview_value,
        "unmanaged": snapshot[2],
    }


def config_file_state(path):
    path = safe_home_path(path)
    target = path.resolve()
    try:
        mode = target.stat().st_mode
    except FileNotFoundError:
        return path, target, b""
    if not stat.S_ISREG(mode):
        raise ValueError("The remote configuration must be a regular file")
    return path, target, target.read_bytes()


def config_file_revision(target, content):
    return hashlib.sha256(os.fsencode(target) + b"\0" + content).hexdigest()


def config_document(path):
    path, target, content = config_file_state(path)
    revision = config_file_revision(target, content)
    content = content.decode("utf-8", errors="replace").replace("\r\n", "\n").replace("\r", "\n")
    return {"path": str(path), "content": content, "revision": revision}


def write_config(path, content, expected_revision):
    path, target, current = config_file_state(path)
    conflict = "The remote configuration changed after it was opened. Reload before saving."
    if config_file_revision(target, current) != expected_revision:
        raise ValueError(conflict)
    replace_remote_config(path, target, current, content.encode("utf-8"), conflict)
    return config_document(path)


def project_summary(project):
    detail = scan_project_data(project)
    return {
        key: detail[key]
        for key in (
            "id", "name", "rootPath", "createdAt", "updatedAt", "lastScannedAt",
            "detectedAgentCount", "skillCount", "mcpCount", "pluginCount",
            "instructionCount", "issueCount",
        )
    }


def scan_project_data(project):
    root = project_root_path(project["rootPath"])
    agents = []
    instructions = []
    health = []
    skill_count = 0
    for spec in AGENTS:
        skill_roots = [
            root / ".agents" / "skills",
            root / spec["skills"],
        ]
        skill_items = []
        seen = set()
        for skills_root in skill_roots:
            for path in skill_directories(skills_root):
                if path.name in seen:
                    continue
                seen.add(path.name)
                meta = frontmatter(path / "SKILL.md", path.name)
                center_skill = center_path() / path.name
                status = "projectOnly"
                if center_skill.is_dir():
                    status = "centerSynced" if hash_dir(center_skill) == hash_dir(path) else "centerDiff"
                skill_items.append({
                    "id": spec["id"] + "::" + path.name,
                    "name": meta["name"],
                    "description": meta["description"],
                    "agentId": spec["id"],
                    "path": str(path),
                    "hash": hash_dir(path),
                    "status": status,
                    "importable": True,
                })
        config_path = root / spec["config"]
        mcp_path = root / spec["mcp"]
        plugin_path = root / spec["plugins"]
        instruction_candidates = [
            root / "AGENTS.md",
            root / "CLAUDE.md",
            root / ".github" / "copilot-instructions.md",
        ]
        existing_instructions = []
        for item in instruction_candidates:
            if item.is_file():
                existing_instructions.append(item)
                instructions.append({
                    "agentId": spec["id"],
                    "path": str(item),
                    "exists": True,
                    "bytes": item.stat().st_size,
                })
        if skill_items or config_path.exists() or mcp_path.exists() or plugin_path.exists():
            agents.append({
                "agentId": spec["id"],
                "displayName": spec["displayName"],
                "iconKey": spec["iconKey"],
                "skillsDirs": [str(item) for item in skill_roots],
                "configPaths": [str(config_path)],
                "mcpConfigPaths": [str(mcp_path)],
                "pluginConfigPaths": [str(plugin_path)],
                "skills": skill_items,
                "mcpServers": [],
                "plugins": [],
                "health": [],
            })
            skill_count += len(skill_items)
    stamp = now()
    return {
        "id": project["id"],
        "name": project["name"],
        "rootPath": str(root),
        "createdAt": project.get("createdAt", stamp),
        "updatedAt": stamp,
        "lastScannedAt": stamp,
        "detectedAgentCount": len(agents),
        "skillCount": skill_count,
        "mcpCount": 0,
        "pluginCount": 0,
        "instructionCount": len(instructions),
        "issueCount": len(health),
        "agents": agents,
        "instructions": instructions,
        "health": health,
    }


def install_to_project(project_id, agent_id, skill_ids, mode):
    project = STATE["projects"].get(project_id)
    if not project:
        raise ValueError("Project not found: " + project_id)
    registered_root = pathlib.Path(project["rootPath"])
    root = project_root_path(registered_root)
    if root != registered_root:
        raise ValueError("The registered project path now points to a different location")
    spec = next((item for item in AGENTS if item["id"] == agent_id), None)
    if not spec:
        raise ValueError("Project-level Skills are not supported for " + agent_id)
    target_root = (root / spec["skills"]).resolve(strict=False)
    if root not in target_root.parents:
        raise ValueError("The Skill directory is outside the registered project")
    for skill_id in skill_ids:
        skill_id = safe_id(skill_id)
        source = center_path() / skill_id
        if not source.is_dir():
            raise ValueError("Skill not found: " + skill_id)
        target = target_root / skill_id
        if target.exists() or target.is_symlink():
            remove_path(target)
        copy_or_link(source, target, mode)
    return scan_project_data(project)


def run_repository_git(root, *args):
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )
    return result.stdout.strip()


def clone_repository(repository, reference=None):
    if not repository:
        raise ValueError("Only GitHub, GitLab, and skills.sh repositories are supported")
    temporary = tempfile.TemporaryDirectory(prefix="agentbro-remote-repo-")
    directory = pathlib.Path(temporary.name)
    root = directory / "repo"
    try:
        if reference is None:
            run_repository_git(directory, "clone", "--depth", "1", "--", repository, str(root))
        else:
            if not reference or reference.startswith(("-", "+")) or ":" in reference:
                raise ValueError("Invalid Git revision")
            root.mkdir()
            run_repository_git(root, "check-ref-format", "--allow-onelevel", reference)
            run_repository_git(root, "init", "--quiet", "--template=")
            run_repository_git(root, "fetch", "--quiet", "--depth=1", "--no-tags",
                               "--no-recurse-submodules", "--", repository, reference)
            run_repository_git(root, "-c", "core.hooksPath=.git/agentbro-no-hooks",
                               "checkout", "--quiet", "--detach", "FETCH_HEAD")
        return root, temporary
    except BaseException:
        temporary.cleanup()
        raise


def repository_relative_path(root, relative):
    path = pathlib.PurePosixPath(relative)
    if path.is_absolute() or ".." in path.parts or "\\" in str(path):
        raise ValueError("Invalid repository Skill path")
    canonical = (root / path).resolve()
    if canonical != root and root not in canonical.parents:
        raise ValueError("Skill path resolves outside the repository")
    return root / path


def repository_skill_directories(root, requested=None, exact=False):
    if not root.is_dir():
        return []
    canonical = root.resolve()
    scoped = repository_relative_path(canonical, requested) if requested and exact else canonical
    if not scoped.is_dir():
        return []
    candidates = [scoped] if (scoped / "SKILL.md").is_file() else [
        path.parent for path in scoped.rglob("SKILL.md")
        if len(path.relative_to(scoped).parts) <= 7
    ]
    if requested and not exact:
        parts = tuple(item for item in requested.split("/") if item)
        candidates = [
            item for item in candidates
            if item.name == parts[-1] or item.relative_to(canonical).parts[-len(parts):] == parts
        ]
    return [
        root / item.relative_to(canonical)
        for item in candidates
        if (item.resolve() == canonical or canonical in item.resolve().parents)
        and not (item / "SKILL.md").is_symlink()
    ]


def repository_info(repo_url, reference=None, revision=None, requested=None):
    clean = str(repo_url).split("#", 1)[0].rstrip("/")
    path = clean
    if clean.startswith("git@"):
        path = clean.split(":", 1)[-1]
    else:
        path = urllib.parse.urlparse(clean).path
    parts = [item for item in path.strip("/").split("/") if item]
    owner = parts[0] if parts else ""
    repo = parts[1] if len(parts) > 1 else ""
    if repo.endswith(".git"):
        repo = repo[:-4]
    return {
        "owner": owner,
        "repo": repo,
        "branch": reference or "HEAD",
        "normalizedUrl": clean,
        "revision": revision,
        "requestedPath": requested,
    }


def github_preview(repo_url):
    repository, requested, reference = resolve_repository_source(repo_url)
    root, temporary = clone_repository(repository, reference)
    try:
        revision = run_repository_git(root, "rev-parse", "HEAD")
        candidates = repository_skill_directories(root, requested, reference is not None)
        center = center_path()
        skills = []
        for path in candidates:
            skill_id = safe_id(path.name)
            meta = frontmatter(path / "SKILL.md", skill_id)
            existing = center / skill_id
            conflict = None
            if existing.exists():
                conflict = {
                    "existingSkillId": skill_id,
                    "existingName": frontmatter(existing / "SKILL.md", skill_id)["name"],
                    "existingCanonicalPath": str(existing),
                    "proposedSkillId": skill_id + "-import",
                    "proposedName": meta["name"],
                }
            skills.append({
                "sourcePath": str(path.relative_to(root)),
                "skillId": skill_id,
                "skillName": meta["name"],
                "description": meta["description"] or None,
                "rootDirectory": str(root),
                "skillDirectoryName": path.name,
                "downloadUrl": repository,
                "conflict": conflict,
            })
        return {"repo": repository_info(repository, reference, revision, requested), "skills": skills}
    finally:
        temporary.cleanup()


def repository_skill_source(repository, reference, relative):
    if not repository.startswith("https://github.com/"):
        return repository
    suffix = "/" + urllib.parse.quote(relative, safe="/") if relative != "." else ""
    base = repository[:-4] if repository.endswith(".git") else repository
    return base + "/tree/" + urllib.parse.quote(reference or "HEAD", safe="") + suffix


def github_import(repo_url, selections, expected_commit=None):
    repository, requested, reference = resolve_repository_source(repo_url)
    if expected_commit is not None and not re.fullmatch(r"(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})", expected_commit):
        raise ValueError("Invalid reviewed Git commit")
    root, temporary = clone_repository(repository, expected_commit or reference)
    try:
        revision = run_repository_git(root, "rev-parse", "HEAD")
        if expected_commit and revision.lower() != expected_commit.lower():
            raise ValueError("The checked out Git commit differs from the preview")
        approved_paths = set(repository_skill_directories(root, requested, reference is not None))
        imported = []
        skipped = []
        for selection in selections:
            relative = str(selection.get("sourcePath") or ".")
            source = repository_relative_path(root.resolve(), relative)
            source = root / source.relative_to(root.resolve())
            if source not in approved_paths:
                raise ValueError("Skill path not found in repository: " + relative)
            original_id = safe_id(source.name)
            resolution = selection.get("resolution", "overwrite")
            if resolution == "skip":
                skipped.append(str(relative))
                continue
            skill_id = safe_id(
                selection.get("renamedSkillId")
                if resolution == "rename"
                else original_id
            )
            target = center_path() / skill_id
            if target.exists() or target.is_symlink():
                if resolution != "overwrite":
                    skipped.append(str(relative))
                    continue
                remove_path(target)
            shutil.copytree(source, target, symlinks=True)
            meta = frontmatter(source / "SKILL.md", original_id)
            imported.append({
                "sourcePath": str(relative),
                "originalSkillId": original_id,
                "importedSkillId": skill_id,
                "skillName": meta["name"],
                "targetDirectory": str(target),
                "resolution": resolution,
            })
            STATE["sources"][skill_id] = {
                "sourceType": "github",
                "sourceUri": repository_skill_source(repository, reference, relative),
                "sourceRef": revision,
                "importedFromAgent": None,
                "importedFromPath": None,
                "installedVia": "remote_git",
                "createdAt": now(),
                "updatedAt": now(),
            }
        save_state()
        return {
            "repo": repository_info(repository, reference, revision, requested),
            "importedSkills": imported,
            "skippedSkills": skipped,
        }
    finally:
        temporary.cleanup()


def read_config_bytes(path):
    try:
        return path.read_bytes()
    except FileNotFoundError:
        return b""


def parse_mcp_config(content):
    message = "Fix the remote MCP JSON before editing this configuration"
    try:
        data = json.loads(content) if content else {}
    except ValueError as error:
        raise ValueError(message) from error
    if not isinstance(data, dict) or not isinstance(data.get("mcpServers", {}), dict):
        raise ValueError(message)
    disabled = data.get("disabledMcpServers", [])
    if not isinstance(disabled, list) or any(not isinstance(name, str) for name in disabled):
        raise ValueError(message)
    for server in data.get("mcpServers", {}).values():
        if isinstance(server, dict) and any(
            server.get(key) and not isinstance(server[key], dict) for key in ("env", "headers")
        ):
            raise ValueError(message)
    return data


def toml_config_parser():
    try:
        import tomllib as parser
    except ImportError:
        try:
            import tomli as parser
        except ImportError:
            return None
    return parser


def parse_codex_mcp_config(content):
    if not content.strip():
        return {}
    parser = toml_config_parser()
    if parser is None:
        raise ValueError("Reading remote Codex MCP TOML requires Python 3.11+ or an installed tomli parser.")

    message = "Fix the remote Codex MCP TOML configuration before inspecting its servers."
    try:
        servers = parser.loads(content.decode("utf-8")).get("mcp_servers", {})
    except ValueError:
        raise ValueError(message) from None
    if not isinstance(servers, dict) or any(not isinstance(server, dict) for server in servers.values()):
        raise ValueError(message)
    return servers


def codex_mcp_connection(config, include_sensitive):
    env = config.get("env", {})
    headers = config.get("http_headers", {})
    header_vars = config.get("env_http_headers", {})
    args = config.get("args", [])
    env_vars = config.get("env_vars", [])
    string_fields = ("command", "cwd", "url", "bearer_token_env_var", "environment_id")
    if (
        any(key in config and not isinstance(config[key], str) for key in string_fields)
        or not isinstance(config.get("enabled", True), bool)
        or not isinstance(args, list)
        or any(not isinstance(arg, str) for arg in args)
        or not isinstance(env_vars, list)
        or any(
            not isinstance(values, dict) or any(not isinstance(value, str) for value in values.values())
            for values in (env, headers, header_vars)
        )
    ):
        return {}, {}, "Invalid remote Codex MCP server configuration."
    if bool(config.get("command")) == bool(config.get("url")):
        return {}, {}, "A remote Codex MCP server requires either a command or a URL."

    env = dict(env)
    headers = {key.title(): value for key, value in headers.items()}
    unsupported = ""
    for entry in env_vars:
        if isinstance(entry, str):
            name = entry
        elif isinstance(entry, dict) and isinstance(entry.get("name"), str):
            name = entry["name"]
            if entry.get("source", "local") != "local":
                unsupported = "This MCP server uses environment variables from a Codex executor."
                continue
        else:
            return {}, {}, "Invalid remote Codex MCP environment variable configuration."
        env.setdefault(name, os.environ.get(name) if include_sensitive else "")

    for key, name in header_vars.items():
        resolved = os.environ.get(name) if include_sensitive else ""
        if resolved is not None:
            headers[key.title()] = resolved
    bearer_var = config.get("bearer_token_env_var")
    if bearer_var:
        bearer = os.environ.get(bearer_var) if include_sensitive else ""
        if bearer is not None:
            headers["Authorization"] = "Bearer " + bearer

    if config.get("environment_id", "local") != "local":
        unsupported = "This MCP server uses a Codex executor."
    elif config.get("http_headers_helper"):
        unsupported = "This MCP server uses a Codex HTTP headers helper."
    return env, headers, unsupported


def mcp_config_values(values, include_sensitive, redact_all=False):
    items = []
    for key, raw in values.items():
        secret = redact_all or bool(re.search(r"authorization|token|key|secret|password|passwd", key, re.I))
        configured = raw is not None
        items.append({
            "key": key,
            "value": str(raw) if configured and (include_sensitive or not secret) else None,
            "secret": secret,
            "configured": configured,
        })
    return items


def resolve_mcp_config_values(draft, previous):
    values = {}
    for item in draft:
        key = item.get("key")
        if not key:
            continue
        value = item.get("value")
        if value == "••••••••" or (value is None and item.get("configured")):
            value = previous.get(key)
            if value is None:
                raise ValueError("No existing sensitive value for " + key)
        values[key] = "" if value is None else value
    return values


def mcp_inventory(agent_id, include_sensitive=False):
    path = agent_mcp_path(agent_id)
    content = read_config_bytes(path)
    revision = hashlib.sha256(content).hexdigest()
    codex = agent_id == "codex"
    editable = not codex and path.suffix == ".json"
    data = parse_mcp_config(content) if editable else {}
    raw_servers = parse_codex_mcp_config(content) if codex else data.get("mcpServers", {})
    disabled = set(data.get("disabledMcpServers", []))
    servers = []
    for name, value in raw_servers.items():
        if not isinstance(value, dict):
            continue
        transport = None if codex else value.get("type")
        if not transport:
            transport = "http" if value.get("url") else "stdio"
        if codex:
            env, headers, message = codex_mcp_connection(value, include_sensitive)
        else:
            env, headers, message = value.get("env") or {}, value.get("headers") or {}, ""
        command, args, cwd, url = (
            value.get("command"), value.get("args", []), value.get("cwd"), value.get("url")
        )
        if codex:
            command = command if isinstance(command, str) else None
            cwd = cwd if isinstance(cwd, str) else None
            url = url if isinstance(url, str) else None
            if not isinstance(args, list) or any(not isinstance(arg, str) for arg in args):
                args = []
        if codex and url:
            try:
                parts = urllib.parse.urlsplit(url)
                if parts.scheme not in ("http", "https") or not parts.hostname or parts.username is not None:
                    message = "The remote Codex MCP server requires an HTTP URL without embedded credentials."
                if not include_sensitive:
                    url = urllib.parse.urlunsplit((parts.scheme, parts.netloc.rsplit("@", 1)[-1], parts.path, "", ""))
            except ValueError:
                url = None
                message = "Invalid remote Codex MCP server URL."
        server = {
            "name": name,
            "transport": transport,
            "command": command,
            "args": args,
            "env": mcp_config_values(env, include_sensitive, redact_all=codex),
            "cwd": cwd,
            "url": url,
            "headers": mcp_config_values(headers, include_sensitive, redact_all=codex),
            "enabled": value.get("enabled", True) is not False if codex else name not in disabled,
            "disabledByAgentbro": name in disabled,
            "valid": not message and bool(command or url),
            "message": message,
            "warnings": [],
            "configPath": str(path),
            "editable": editable,
            "sourceKind": "remote_config",
        }
        if codex and include_sensitive:
            server["codexConnection"] = True
        servers.append(server)
    return {
        "agentId": agent_id,
        "configPath": str(path),
        "revision": revision,
        "capabilities": {
            "editable": editable,
            "supportsStdio": True,
            "supportsHttp": True,
            "supportsSse": not codex,
            "supportsNativeToggle": editable,
        },
        "servers": servers,
    }


def write_mcp_inventory(
    agent_id, revision, server=None, original_name=None, delete_name=None, toggle=None
):
    path = agent_mcp_path(agent_id)
    if agent_id == "codex" or path.suffix != ".json":
        raise ValueError("This remote Agent MCP format is read-only")
    target = path.resolve()
    content = read_config_bytes(target)
    conflict = "The remote MCP configuration changed after it was opened. Reload before saving."
    if hashlib.sha256(content).hexdigest() != revision:
        raise ValueError(conflict)
    data = parse_mcp_config(content)
    servers = data.setdefault("mcpServers", {})
    disabled = set(data.get("disabledMcpServers", []))
    if delete_name:
        servers.pop(delete_name, None)
        disabled.discard(delete_name)
    elif toggle:
        name, enabled = toggle
        if enabled:
            disabled.discard(name)
        else:
            disabled.add(name)
    elif server:
        name = safe_id(server["name"])
        if name in servers and name != original_name:
            raise ValueError("An MCP server with that name already exists")
        previous = servers.get(original_name or name)
        previous = previous if isinstance(previous, dict) else {}
        value = dict(previous)
        if original_name and original_name != name:
            servers.pop(original_name, None)
            if original_name in disabled:
                disabled.remove(original_name)
                disabled.add(name)
        value["type"] = server.get("transport", "stdio")
        for key in ("command", "args", "cwd", "url", "env", "headers"):
            value.pop(key, None)
        for key in ("command", "args", "cwd", "url"):
            if server.get(key) not in (None, "", []):
                value[key] = server[key]
        for key in ("env", "headers"):
            if server.get(key):
                value[key] = resolve_mcp_config_values(server[key], previous.get(key) or {})
        servers[name] = value
    data["disabledMcpServers"] = sorted(disabled)
    updated = json.dumps(data, ensure_ascii=False, indent=2).encode()
    replace_remote_config(path, target, content, updated, conflict)
    return mcp_inventory(agent_id)


def replace_remote_config(path, target, content, updated, conflict):
    target.parent.mkdir(parents=True, exist_ok=True)
    temp_path = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb", dir=target.parent, suffix=".tmp", delete=False
        ) as temp:
            temp_path = pathlib.Path(temp.name)
            if target.is_file():
                temp_path.chmod(stat.S_IMODE(target.stat().st_mode))
            temp.write(updated)
            temp.flush()
            os.fsync(temp.fileno())
        if path.resolve() != target or read_config_bytes(target) != content:
            raise ValueError(conflict)
        temp_path.replace(target)
    finally:
        if temp_path is not None:
            unlink_if_exists(temp_path)


def mcp_server(agent_id, server_name):
    inventory_value = mcp_inventory(agent_id, include_sensitive=True)
    server = next(
        (item for item in inventory_value["servers"] if item["name"] == server_name),
        None,
    )
    if not server:
        raise ValueError("MCP server not found: " + server_name)
    if not server["enabled"]:
        raise ValueError("MCP server is disabled")
    if agent_id == "codex" and not server["valid"]:
        raise ValueError(server["message"] or "The remote Codex MCP server requires a command or URL.")
    return server


def mcp_cancel_marker(kind, request_id):
    owner = os.getuid()
    scope = hashlib.sha256(os.fsencode(HOME)).hexdigest()[:16]
    root = pathlib.Path(tempfile.gettempdir()) / "agentbro-mcp-cancel-{}-{}".format(owner, scope)
    root.mkdir(mode=0o700, exist_ok=True)
    info = root.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != owner or info.st_mode & 0o077:
        raise RuntimeError("MCP cancellation directory must be private")

    # Separate SSH connections can deliver cancellation before the operation starts.
    cutoff = time.time() - 180
    for entry in root.iterdir():
        try:
            if re.fullmatch(r"[0-9a-f]{64}", entry.name):
                info = entry.lstat()
                if stat.S_ISREG(info.st_mode) and info.st_mtime < cutoff:
                    entry.unlink()
        except FileNotFoundError:
            pass
    key = json.dumps([kind, request_id]).encode()
    return root / hashlib.sha256(key).hexdigest()


def cancel_mcp_request(kind, request_id):
    marker = mcp_cancel_marker(kind, request_id)
    try:
        fd = os.open(marker, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError:
        return
    os.close(fd)


def stop_mcp_worker(_signal, _frame):
    raise SystemExit(1)


def supervise_mcp_request(kind, request_id):
    marker = mcp_cancel_marker(kind, request_id)
    cancelled = "MCP " + kind + " cancelled"
    if marker.is_file():
        unlink_if_exists(marker)
        raise RuntimeError(cancelled)

    reader, writer = os.pipe()
    try:
        worker = os.fork()
    except OSError:
        os.close(reader)
        os.close(writer)
        raise
    if worker == 0:
        os.close(reader)
        signal.signal(signal.SIGUSR1, stop_mcp_worker)
        os.setpgid(0, 0)
        return

    os.close(writer)
    try:
        try:
            os.setpgid(worker, worker)
        except ProcessLookupError:
            pass
        # Poll for a separate SSH request while the worker can block in either transport.
        while not select.select([reader], [], [], 0.1)[0]:
            if marker.is_file():
                try:
                    os.kill(worker, signal.SIGUSR1)
                except ProcessLookupError:
                    pass
                select.select([reader], [], [], 2.5)
                raise RuntimeError(cancelled)
        _, status = os.waitpid(worker, 0)
        worker = None
        if os.WIFEXITED(status):
            raise SystemExit(os.WEXITSTATUS(status))
        raise SystemExit(128 + os.WTERMSIG(status))
    finally:
        try:
            if worker is not None:
                try:
                    os.killpg(worker, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                os.waitpid(worker, 0)
        finally:
            os.close(reader)
            unlink_if_exists(marker)


def read_json_line(descriptor, pending, deadline):
    scanned = 0
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("MCP server response timed out")
        end = pending.find(b"\n", scanned)
        if end >= 0:
            line = pending[:end]
            del pending[:end + 1]
            scanned = 0
            try:
                value = json.loads(line.decode("utf-8"))
            except (ValueError, RecursionError):
                continue
            if isinstance(value, dict):
                return value
            continue
        scanned = len(pending)
        ready, _, _ = select.select([descriptor], [], [], remaining)
        if not ready:
            raise TimeoutError("MCP server response timed out")
        chunk = os.read(descriptor, 65536)
        if not chunk:
            raise ValueError("MCP process exited before responding")
        pending.extend(chunk)


@contextmanager
def stdio_mcp_session(server):
    command = server.get("command")
    if not command:
        raise ValueError("MCP stdio server has no command")
    environment = os.environ.copy()
    if server.get("codexConnection"):
        environment = {
            name: environment[name] for name in (
                "HOME", "LANG", "LC_ALL", "LOGNAME", "PATH", "SHELL", "TERM",
                "TMPDIR", "TZ", "USER", "__CF_USER_TEXT_ENCODING",
            ) if name in environment
        }
    for item in server.get("env", []):
        if item.get("key") and item.get("value") is not None:
            environment[item["key"]] = item["value"]
    process = subprocess.Popen(
        [command] + [str(item) for item in server.get("args", [])],
        cwd=server.get("cwd") or None,
        env=environment,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    stdout = process.stdout.fileno()
    pending = bytearray()

    def send(value):
        line = json.dumps(value, separators=(",", ":")) + "\n"
        process.stdin.write(line.encode("utf-8"))
        process.stdin.flush()

    request_ids = count(1)

    def request(method, params=None, *, timeout=15):
        request_id = next(request_ids)
        payload = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
        }
        if params is not None:
            payload["params"] = params
        send(payload)
        deadline = time.monotonic() + timeout
        while True:
            response = read_json_line(stdout, pending, deadline)
            if response.get("id") == request_id:
                if response.get("error"):
                    raise ValueError(json.dumps(response["error"], ensure_ascii=False))
                return response.get("result")

    try:
        initialized = request("initialize", {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "AgentBro", "version": "remote"},
        })
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        yield initialized, request
    finally:
        process.terminate()
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def mcp_event_lines(response, deadline):
    pending = bytearray()
    trailing_cr = False
    while time.monotonic() < deadline:
        chunk = response.read1(8192)
        if not chunk:
            return
        start = int(trailing_cr and chunk.startswith(b"\n"))
        trailing_cr = chunk.endswith(b"\r")
        for match in MCP_SSE_LINE_END.finditer(chunk, start):
            pending.extend(chunk[start:match.start()])
            yield bytes(pending)
            pending.clear()
            start = match.end()
        pending.extend(chunk[start:])
    raise TimeoutError("MCP event stream response timed out")


def read_mcp_event_response(response, request_id, deadline):
    data = []
    for index, line in enumerate(mcp_event_lines(response, deadline)):
        if index == 0 and line.startswith(b"\xef\xbb\xbf"):
            line = line[3:]
        if not line and data:
            payload = b"\n".join(data)
            data.clear()
            try:
                value = json.loads(payload.decode("utf-8"))
            except (ValueError, RecursionError):
                continue
            if (
                isinstance(value, dict)
                and value.get("id") == request_id
                and "method" not in value
                and ("result" in value or "error" in value)
            ):
                return value
        else:
            field, _, text = line.partition(b":")
            if field == b"data":
                data.append(text[1:] if text.startswith(b" ") else text)
    raise ValueError("MCP event stream closed before the request received a response")


def http_mcp_exchange(url, headers, payload=None, session_id=None, protocol_version=None, *, timeout=20):
    has_payload = payload is not None
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode() if has_payload else None,
        headers={"Content-Type": "application/json", "Accept": "application/json, text/event-stream"},
        method="POST" if has_payload else "DELETE",
    )
    for item in headers:
        key = item.get("key")
        if key and key.lower() not in ("mcp-session-id", "mcp-protocol-version") and item.get("value") is not None:
            request.add_unredirected_header(key, item["value"])
    if session_id:
        request.add_unredirected_header("Mcp-Session-Id", session_id)
    if protocol_version:
        request.add_unredirected_header("MCP-Protocol-Version", protocol_version)

    deadline = time.monotonic() + timeout
    with urllib.request.urlopen(request, timeout=timeout) as response:
        if not has_payload:
            return None, session_id
        session = response.headers.get("Mcp-Session-Id") or session_id
        if "id" in payload and response.headers.get_content_type() == "text/event-stream":
            value = read_mcp_event_response(response, payload["id"], deadline)
        else:
            raw = response.read().decode(errors="replace")
            if raw.startswith("event:") or "\ndata:" in raw:
                data_lines = [
                    line[5:].strip() for line in raw.splitlines()
                    if line.startswith("data:")
                ]
                raw = data_lines[-1] if data_lines else "{}"
            value = json.loads(raw or "{}")
    if value.get("error"):
        raise ValueError(json.dumps(value["error"], ensure_ascii=False))
    return value.get("result"), session


@contextmanager
def http_mcp_session(server):
    url = server.get("url")
    if not url:
        raise ValueError("Remote MCP server has no URL")
    headers = server.get("headers", [])
    initialized, session_id = http_mcp_exchange(
        url,
        headers,
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "AgentBro", "version": "remote"},
            },
        },
    )
    protocol_version = "2025-03-26"
    try:
        if initialized is not None and not isinstance(initialized, dict):
            raise ValueError("Invalid remote MCP initialization result.")
        negotiated = (initialized or {}).get("protocolVersion", protocol_version)
        if not isinstance(negotiated, str) or not re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", negotiated):
            raise ValueError("Invalid remote MCP protocol version.")
        protocol_version = negotiated
        http_mcp_exchange(
            url,
            headers,
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            session_id=session_id,
            protocol_version=protocol_version,
        )
        request_ids = count(2)

        def request(method, params=None, *, timeout=20):
            result, _ = http_mcp_exchange(
                url,
                headers,
                {"jsonrpc": "2.0", "id": next(request_ids), "method": method, "params": params or {}},
                session_id=session_id,
                protocol_version=protocol_version,
                timeout=timeout,
            )
            return result

        yield initialized, request
    finally:
        if session_id:
            try:
                http_mcp_exchange(
                    url, headers, session_id=session_id, protocol_version=protocol_version, timeout=1
                )
            except (OSError, ValueError):
                # Remote peers may reject DELETE or be unavailable after cancellation.
                pass


def mcp_error_message(server, error):
    message = str(error)
    if not server.get("codexConnection"):
        return message
    values = {item["value"] for item in server["env"] + server["headers"] if item["value"]}
    values.update(value[7:] for value in list(values) if value.lower().startswith("bearer "))
    if server["url"]:
        query = urllib.parse.urlsplit(server["url"]).query
        values.update(value for _, value in urllib.parse.parse_qsl(query) if value)
    for value in sorted(values, key=len, reverse=True):
        if value:
            message = message.replace(value, "***")
    return message


@contextmanager
def mcp_session(server):
    transport = stdio_mcp_session if server["transport"] == "stdio" else http_mcp_session
    try:
        with transport(server) as (initialized, request):
            if initialized is not None and not isinstance(initialized, dict):
                raise ValueError("Invalid remote MCP initialization result.")
            yield initialized or {}, request
    except Exception as error:
        if not server.get("codexConnection"):
            raise
        raise ValueError(mcp_error_message(server, error)) from None


def mcp_request(agent_id, server_name, method, params=None):
    server = mcp_server(agent_id, server_name)
    with mcp_session(server) as (_, request):
        return request(method, params)


def advertised_mcp_capabilities(initialized):
    capabilities = initialized.get("capabilities")
    return capabilities if isinstance(capabilities, dict) else None


def discover_mcp_catalog(request, capability):
    entries = []
    seen_cursors = set()
    params = {}
    deadline = time.monotonic() + MCP_INSPECTION_CATALOG_TIMEOUT
    for page in range(MCP_INSPECTION_MAX_PAGES):
        try:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("Catalog discovery timed out")
            result = request(capability + "/list", params, timeout=remaining)
            page_entries = result.get(capability) if isinstance(result, dict) else None
            if not isinstance(page_entries, list) or any(not isinstance(item, dict) for item in page_entries):
                raise ValueError("Invalid remote MCP catalog.")
        except Exception as error:
            return (entries if page else None), error

        space = MCP_INSPECTION_MAX_ITEMS - len(entries)
        entries.extend(page_entries[:space])
        cursor = result.get("nextCursor")
        if len(page_entries) > space or (cursor is not None and len(entries) == MCP_INSPECTION_MAX_ITEMS):
            return entries, "Catalog stopped at the {}-item limit".format(MCP_INSPECTION_MAX_ITEMS)
        if cursor is None:
            return entries, None
        if not isinstance(cursor, str):
            return entries, "Server returned an invalid pagination cursor"
        if cursor in seen_cursors:
            return entries, "Server repeated a pagination cursor"
        seen_cursors.add(cursor)
        params = {"cursor": cursor}
    return entries, "Catalog stopped at the {}-page limit".format(MCP_INSPECTION_MAX_PAGES)


def inspect_mcp(agent_id, server_name, inspection_id):
    started = time.time()
    server = mcp_server(agent_id, server_name)
    warnings = []
    catalogs = {}
    with mcp_session(server) as (initialized, request):
        advertised = advertised_mcp_capabilities(initialized)
        for capability in ("tools", "resources", "prompts"):
            if advertised is not None and capability not in advertised:
                continue
            entries, warning = discover_mcp_catalog(request, capability)
            if entries is not None:
                catalogs[capability] = entries
            if warning is not None:
                warnings.append(capability + "/list: " + mcp_error_message(server, warning))
    available = advertised if advertised is not None else catalogs
    server_info = initialized.get("serverInfo", {})
    return {
        "inspectionId": inspection_id,
        "status": "partial" if warnings else "connected",
        "category": "success",
        "summary": "Remote MCP inspection completed",
        "inspectedAtMs": int(time.time() * 1000),
        "durationMs": int((time.time() - started) * 1000),
        "protocolVersion": initialized.get("protocolVersion"),
        "serverName": server_info.get("name"),
        "serverVersion": server_info.get("version"),
        "transport": server["transport"],
        "capabilities": {
            name: name in available for name in ("tools", "resources", "prompts", "logging")
        },
        "tools": [
            {
                "name": item.get("name", ""),
                "title": item.get("title"),
                "description": item.get("description"),
                "inputs": [],
                "inputSchema": item.get("inputSchema", {}),
                "outputSchema": item.get("outputSchema"),
                "annotations": {
                    "readOnly": (item.get("annotations") or {}).get("readOnlyHint"),
                    "destructive": (item.get("annotations") or {}).get("destructiveHint"),
                    "idempotent": (item.get("annotations") or {}).get("idempotentHint"),
                    "openWorld": (item.get("annotations") or {}).get("openWorldHint"),
                },
                "hasAnnotations": bool(item.get("annotations")),
            }
            for item in catalogs.get("tools", [])
        ],
        "resources": [
            {
                "uri": item.get("uri", ""),
                "name": item.get("name", ""),
                "title": item.get("title"),
                "description": item.get("description"),
                "mimeType": item.get("mimeType"),
                "size": item.get("size"),
            }
            for item in catalogs.get("resources", [])
        ],
        "prompts": [
            {
                "name": item.get("name", ""),
                "title": item.get("title"),
                "description": item.get("description"),
                "arguments": item.get("arguments", []),
            }
            for item in catalogs.get("prompts", [])
        ],
        "steps": [],
        "warnings": warnings,
        "suggestions": [],
    }


def read_plugin_json(path):
    try:
        document = json.loads(path.read_bytes())
        return document if isinstance(document, dict) else {}
    except (OSError, ValueError):
        return {}


def plugin_manifest_path(agent_id, directory):
    if agent_id == "codex":
        return directory / ".codex-plugin" / "plugin.json"
    if is_claude_agent(agent_id):
        return directory / ".claude-plugin" / "plugin.json"
    return directory / "package.json"


def plugin_configuration(agent_id):
    if agent_id != "codex" and not is_claude_agent(agent_id):
        return None
    try:
        path = safe_home_path(agent_config_path(agent_id))
        target = path.resolve()
        content = read_config_bytes(target)
        if agent_id == "codex":
            parser = toml_config_parser()
            if parser is None:
                return None
            document = parser.loads(content.decode("utf-8"))
            plugins = document.get("plugins", {})
            if not isinstance(plugins, dict) or any(
                not isinstance(item, dict) or not isinstance(item.get("enabled", True), bool)
                for item in plugins.values()
            ):
                return None
            states = {key: item.get("enabled", True) for key, item in plugins.items()}
        else:
            document = json.loads(content) if content.strip() else {}
            if not isinstance(document, dict):
                return None
            states = document.get("enabledPlugins", {})
            if not isinstance(states, dict) or any(not isinstance(value, bool) for value in states.values()):
                return None
    except (OSError, ValueError, RuntimeError):
        return None
    return path, target, content, document, states


def discovered_plugins(agent_id, root):
    records = {}
    native = agent_id == "codex" or is_claude_agent(agent_id)
    if not root.is_dir():
        return records
    directories = root.glob("*/*/*") if native else root.iterdir()
    for directory in sorted(directories):
        if not directory.is_dir():
            continue
        manifest = plugin_manifest_path(agent_id, directory)
        resolved = manifest.resolve()
        if root not in resolved.parents:
            continue
        metadata = read_plugin_json(manifest)
        if native:
            name = metadata.get("name")
            if not isinstance(name, str) or not name:
                continue
            plugin_id = f"{name}@{directory.parent.parent.name}"
        else:
            plugin_id = directory.name
        current = records.get(plugin_id)
        if current is None or current.name < directory.name:
            records[plugin_id] = directory
    return records


def claude_user_plugins(config):
    registry = read_plugin_json(config.parent / "plugins" / "installed_plugins.json").get("plugins", {})
    if not isinstance(registry, dict):
        return set()
    return {
        plugin_id for plugin_id, entries in registry.items()
        if any(
            isinstance(entry, dict) and entry.get("scope", "user") == "user"
            for entry in (entries if isinstance(entries, list) else [entries])
        )
    }


def plugin_inventory(agent_id, configuration=None):
    if not agent_spec(agent_id):
        raise ValueError("Unknown Agent: " + agent_id)
    native = agent_id == "codex" or is_claude_agent(agent_id)
    try:
        root = safe_home_path(agent_plugin_path(agent_id)).resolve()
        records = discovered_plugins(agent_id, root)
    except (OSError, ValueError, RuntimeError):
        records = {}
    path = agent_config_path(agent_id) if native else None
    configuration = configuration if configuration is not None else plugin_configuration(agent_id)
    states = configuration[4] if configuration else {}
    ids = set(states) if configuration else set(records)
    if is_claude_agent(agent_id) and configuration:
        ids.update(claude_user_plugins(path))

    plugins = []
    for plugin_id in sorted(ids, key=lambda value: (value.lower(), value)):
        fallback_name = plugin_id.partition("@")[0]
        directory = records.get(plugin_id)
        metadata = read_plugin_json(plugin_manifest_path(agent_id, directory)) if directory else {}
        interface = metadata.get("interface")
        display_name = interface.get("displayName") if isinstance(interface, dict) else None
        name = next((value for value in (display_name, metadata.get("displayName"), metadata.get("name"))
                     if isinstance(value, str) and value), fallback_name)
        version = metadata.get("version")
        plugins.append({
            "id": plugin_id,
            "name": name,
            "version": version if isinstance(version, str) else None,
            "enabled": states.get(plugin_id, True) if configuration else None,
            "source": str(directory) if directory else None,
        })
    return {
        "agentId": agent_id,
        "configPath": str(path) if path else None,
        "revision": config_file_revision(configuration[1], configuration[2]) if configuration else "read-only",
        "capabilities": {"editable": configuration is not None, "requiresNewSession": True},
        "plugins": plugins,
    }


def update_codex_plugin_setting(content, document, plugin_id, enabled):
    parser = toml_config_parser()
    text = content.decode("utf-8")
    plugin = document["plugins"][plugin_id]
    previous = plugin.get("enabled")
    plugin["enabled"] = enabled
    value = "true" if enabled else "false"

    # Match the parsed document so comments and strings cannot be mistaken for the setting.
    def verified(candidate):
        try:
            return parser.loads(candidate) == document
        except ValueError:
            return False

    if previous is not None:
        old_value = "true" if previous else "false"
        for match in re.finditer(r"\b" + old_value + r"\b", text):
            candidate = text[:match.start()] + value + text[match.end():]
            if verified(candidate):
                return candidate.encode()
    else:
        newline = "\r\n" if "\r\n" in text else "\n"
        assignment = "enabled = " + value
        key = json.dumps(plugin_id, ensure_ascii=False)
        offset = 0
        for line in text.splitlines(keepends=True):
            offset += len(line)
            if not line.lstrip().startswith("["):
                continue
            try:
                table = parser.loads(line)
            except ValueError:
                continue
            if table == {"plugins": {plugin_id: {}}}:
                entry = assignment
            elif table == {"plugins": {}}:
                entry = key + "." + assignment
            else:
                continue
            separator = "" if line.endswith(("\n", "\r")) else newline
            candidate = text[:offset] + separator + entry + newline + text[offset:]
            if verified(candidate):
                return candidate.encode()
        for match in re.finditer(r"\{", text):
            index = match.end()
            separator = "" if text[index:].lstrip().startswith("}") else ", "
            candidate = text[:index] + assignment + separator + text[index:]
            if verified(candidate):
                return candidate.encode()
        candidate = "plugins." + key + "." + assignment + newline + text
        if verified(candidate):
            return candidate.encode()
        candidate = text + newline + "[plugins." + key + "]" + newline + assignment + newline
        if verified(candidate):
            return candidate.encode()
    raise ValueError("Could not update this Codex plugin without changing other settings.")


def set_plugin_enabled(agent_id, plugin_id, revision, enabled):
    configuration = plugin_configuration(agent_id)
    if configuration is None:
        raise ValueError("This remote Agent's plugin settings are read-only.")
    path, target, content, document, _states = configuration
    conflict = "The remote plugin settings changed. Reload before saving."
    if revision != config_file_revision(target, content):
        raise ValueError(conflict)
    if not isinstance(enabled, bool):
        raise ValueError("Plugin enablement must be true or false.")
    inventory_value = plugin_inventory(agent_id, configuration)
    plugin = next((item for item in inventory_value["plugins"] if item["id"] == plugin_id), None)
    if plugin is None:
        raise ValueError("Plugin is no longer configured. Reload the plugin list.")
    if plugin["enabled"] == enabled:
        return inventory_value
    if agent_id == "codex":
        updated = update_codex_plugin_setting(content, document, plugin_id, enabled)
    else:
        document.setdefault("enabledPlugins", {})[plugin_id] = enabled
        updated = (json.dumps(document, ensure_ascii=False, indent=2) + "\n").encode()
    replace_remote_config(path, target, content, updated, conflict)
    return plugin_inventory(agent_id)


def plugin_file_tree(path, root):
    if path.is_symlink():
        return {
            "name": path.name,
            "nodeType": "symlink",
            "path": str(path.relative_to(root)),
            "children": None,
            "omittedCount": None,
        }
    if path.is_file():
        return {
            "name": path.name,
            "nodeType": "file",
            "path": str(path.relative_to(root)),
            "children": None,
            "omittedCount": None,
        }
    children = [
        plugin_file_tree(item, root)
        for item in sorted(path.iterdir(), key=lambda value: value.name.lower())
        if item.name != ".git"
    ]
    return {
        "name": path.name,
        "nodeType": "directory",
        "path": str(path.relative_to(root)) if path != root else "",
        "children": children,
        "omittedCount": None,
    }


def plugin_detail(agent_id, plugin_id):
    inventory_value = plugin_inventory(agent_id)
    plugin = next(
        (item for item in inventory_value["plugins"] if item["id"] == plugin_id),
        None,
    )
    if not plugin:
        raise ValueError("Plugin not found: " + plugin_id)
    if not plugin["source"]:
        raise ValueError("Installed files are not available for this plugin.")
    root = safe_home_path(plugin["source"], allow_missing=False).resolve()
    manifest = plugin_manifest_path(agent_id, root)
    metadata = read_plugin_json(manifest)
    file_count = sum(1 for item in root.rglob("*") if item.is_file())
    return {
        **plugin,
        "description": metadata.get("description"),
        "author": metadata.get("author") if isinstance(metadata.get("author"), str) else None,
        "homepage": metadata.get("homepage"),
        "license": metadata.get("license"),
        "installPath": str(root),
        "manifestPath": str(manifest) if manifest.is_file() else None,
        "files": plugin_file_tree(root, root),
        "fileCount": file_count,
        "truncated": False,
    }


def plugin_file(agent_id, plugin_id, relative_path):
    detail = plugin_detail(agent_id, plugin_id)
    root = pathlib.Path(detail["installPath"])
    relative = pathlib.Path(relative_path)
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError("Invalid plugin file path")
    path = (root / relative).resolve()
    if root not in path.parents or not path.is_file():
        raise ValueError("Plugin file not found")
    data = path.read_bytes()
    truncated = len(data) > 2 * 1024 * 1024
    data = data[:2 * 1024 * 1024]
    try:
        content = data.decode("utf-8")
        kind = "text"
        encoded = None
    except UnicodeDecodeError:
        content = None
        kind = "binary"
        encoded = base64.b64encode(data).decode()
    return {
        "path": str(relative),
        "kind": kind,
        "mimeType": None,
        "content": content,
        "dataBase64": encoded,
        "size": path.stat().st_size,
        "truncated": truncated,
    }


def move_direct_preview(target_id, pack_id):
    skills, agents, _, targets = inventory()
    target = targets.get(target_id)
    if not target:
        raise ValueError("Target not found: " + target_id)
    if not any(
        claim["claimType"] == "direct" or claim["packId"] == pack_id
        for claim in target["claims"]
    ):
        raise ValueError("Target does not have a direct Skill distribution")
    detail = pack_detail(pack_id, skills, agents)
    skill = skills[target["skillId"]]
    member_ids = [item["skillId"] for item in detail["members"] if not item["missing"]]
    already_member = target["skillId"] in member_ids
    already_applied = target["agentId"] in [
        item["agentId"] for item in detail["appliedAgents"]
    ]
    remaining = [item for item in member_ids if item != target["skillId"]]
    distribution = distribution_preview([], [target["agentId"]], target["actualMode"])
    return {
        "targetId": target_id,
        "skillId": target["skillId"],
        "skillName": skill["name"],
        "agentId": target["agentId"],
        "displayName": agent_spec(target["agentId"])["displayName"],
        "packId": pack_id,
        "packName": detail["name"],
        "alreadyMember": already_member,
        "alreadyApplied": already_applied,
        "willAddToPack": pack_id != "default" and not already_member,
        "otherMemberCount": len(remaining),
        "distribution": distribution,
    }


def command_version(binary):
    try:
        result = subprocess.run(
            [binary, "--version"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=8,
        )
        return result.stdout.strip().splitlines()[0][:120] if result.stdout.strip() else None
    except Exception:
        return None


def agent_programs():
    package_data = {
        "claude-code": ("npm", "@anthropic-ai/claude-code"),
        "codex": ("npm", "@openai/codex"),
        "gemini": ("npm", "@google/gemini-cli"),
        "opencode": ("npm", "opencode-ai"),
        "openclaw": ("npm", "openclaw"),
        "copilot": ("npm", "@github/copilot"),
        "qwen": ("npm", "@qwen-code/qwen-code"),
        "kimi": ("npm", "@moonshot-ai/kimi-code"),
        "aider": ("uv", "aider-chat"),
    }
    result = []
    for spec in AGENTS:
        binary = agent_binary(spec)
        package_manager, package_name = package_data.get(spec["id"], (None, None))
        installed = agent_installed(spec, binary)
        config_path = agent_config_path(spec["id"])
        config_text = config_path.read_text(errors="replace") if config_path.is_file() else ""
        hooks_installed = (
            (HOME / ".agentbro" / "remote" / "hook.py").is_file()
            and ("remote/hook.py" in config_text or "remote-hook.py" in config_text)
        )
        result.append({
            "id": spec["id"],
            "displayName": spec["displayName"],
            "icon": spec["iconKey"],
            "kind": "cli",
            "status": "installed" if installed else "notInstalled",
            "packageManager": package_manager,
            "packageName": package_name,
            "installedVersion": command_version(binary) if binary else None,
            "latestVersion": None,
            "binaryPath": binary,
            "configDir": str(agent_config_path(spec["id"]).parent),
            "appPath": None,
            "downloadUrl": None,
            "installCommand": (
                "npm install -g " + package_name
                if package_manager == "npm"
                else "uv tool install " + package_name
                if package_manager == "uv"
                else None
            ),
            "updateCommand": (
                "npm install -g " + package_name + "@latest"
                if package_manager == "npm"
                else "uv tool upgrade " + package_name
                if package_manager == "uv"
                else None
            ),
            "uninstallCommand": (
                "npm uninstall -g " + package_name
                if package_manager == "npm"
                else "uv tool uninstall " + package_name
                if package_manager == "uv"
                else None
            ),
            "hooksInstalled": hooks_installed,
            "skillsDir": str(agent_skills_path(spec["id"])),
            "isCustom": False,
        })
    for agent_id, record in STATE["customAgents"].items():
        spec = agent_spec(agent_id)
        if spec and "configDir" in spec:
            program = {key: value for key, value in record.items()
                       if key not in ("category", "settingsFile", "mcpConfig", "pluginDir")}
            program["configDir"] = str(spec["configDir"])
            program["skillsDir"] = str(spec["skills"])
            program["status"] = "installed" if agent_installed(spec) else "notInstalled"
            result.append(program)
    return result


def custom_agent_program(agent_id, config):
    name = config["displayName"].strip()
    skills_dir = config["globalSkillsDir"].strip()
    if not name or not skills_dir:
        raise ValueError("Agent display name and Skills directory are required")
    root = config.get("configDir")
    if root and not safe_home_path(root).is_dir():
        raise ValueError("The Agent configuration directory does not exist on this SSH host")
    value = {
        "id": agent_id,
        "displayName": name,
        "icon": config.get("iconName") or "custom",
        "kind": "cli",
        "packageManager": "custom",
        "packageName": None,
        "installedVersion": None,
        "latestVersion": None,
        "binaryPath": None,
        "configDir": root or skills_dir,
        "appPath": None,
        "downloadUrl": None,
        "installCommand": None,
        "updateCommand": None,
        "uninstallCommand": None,
        "hooksInstalled": False,
        "skillsDir": skills_dir,
        "isCustom": True,
        "category": config.get("category") or "custom",
        "settingsFile": config.get("settingsFile"),
        "mcpConfig": config.get("mcpConfig"),
        "pluginDir": config.get("pluginDir"),
    }
    spec = custom_agent_spec(value)
    value["status"] = "installed" if agent_installed(spec) else "notInstalled"
    return value


def run_agent_package_operation(agent_id, operation):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + agent_id)
    packages = {
        "claude-code": ("npm", "@anthropic-ai/claude-code"),
        "codex": ("npm", "@openai/codex"),
        "gemini": ("npm", "@google/gemini-cli"),
        "opencode": ("npm", "opencode-ai"),
        "openclaw": ("npm", "openclaw"),
        "copilot": ("npm", "@github/copilot"),
        "qwen": ("npm", "@qwen-code/qwen-code"),
        "kimi": ("npm", "@moonshot-ai/kimi-code"),
        "aider": ("uv", "aider-chat"),
    }
    package = packages.get(agent_id)
    if not package:
        raise ValueError("This Agent does not support remote package management")
    manager, name = package
    if manager == "npm":
        executable = shutil.which("npm")
        if not executable:
            raise ValueError("npm is not installed on the remote server")
        if operation == "uninstall":
            command = [executable, "uninstall", "-g", name]
        else:
            package_name = name + "@latest" if operation == "update" else name
            command = [executable, "install", "-g", package_name]
    else:
        executable = shutil.which("uv")
        if not executable:
            raise ValueError("uv is not installed on the remote server")
        uv_operation = "upgrade" if operation == "update" else operation
        command = [executable, "tool", uv_operation, name]
    completed = subprocess.run(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=300,
    )
    if completed.returncode != 0:
        raise ValueError((completed.stderr or completed.stdout)[-1200:])
    return None


def dispatch():
    command = COMMAND
    args = ARGS
    if command in ("skill_manager_bootstrap", "skill_manager_init", "skill_manager_refresh"):
        migrate_configured_center()
        center_path()
        save_state()
        return None
    if command in ("skill_manager_overview", "skill_manager_refresh_overview"):
        return overview()
    if command == "skill_pack_picker_data":
        data = overview()
        return {
            "agents": data["agents"],
            "packs": data["packs"],
            "appliedByAgent": {
                agent["id"]: applied_pack_ids(agent["id"])
                for agent in data["agents"]
            },
            "defaultDistributeMode": STATE["settings"]["defaultDistributeMode"],
        }
    if command == "skill_manager_settings":
        return STATE["settings"]
    if command == "skill_manager_update_settings":
        update = args.get("update") or {}
        mapping = {
            "sqlitePath": "sqlitePath",
            "defaultDistributeMode": "defaultDistributeMode",
            "linkFailPolicy": "linkFailPolicy",
            "startupScan": "startupScan",
            "showUnmanaged": "showUnmanaged",
            "autoSyncSkillPacks": "autoSyncSkillPacks",
        }
        for source, target in mapping.items():
            if update.get(source) is not None:
                STATE["settings"][target] = update[source]
        STATE["settings"]["centerPath"] = str(FIXED_CENTER_PATH)
        center_path()
        save_state()
        return STATE["settings"]
    if command == "list_center_skills_v2":
        return overview()["skills"]
    if command == "get_skill_detail_v2":
        return skill_detail(args["skillId"])
    if command == "read_skill_files":
        return file_tree(args["skillPath"])
    if command == "read_skill_file_content":
        path = safe_home_path(args["filePath"], allow_missing=False)
        if path.stat().st_size > 2 * 1024 * 1024:
            raise ValueError("Remote file is too large to preview")
        return path.read_text(errors="replace")
    if command == "browse_remote_skill_sources":
        return browse_skill_sources(args.get("path"))
    if command == "preview_add_center_skill":
        return preview_add(args["input"])
    if command == "execute_add_center_skill":
        return execute_add(args["input"], args.get("decisions", []))
    if command == "execute_marketplace_skill_batch":
        results = []
        for item in args.get("skills", []):
            try:
                source = item.get("sourceUri") or ""
                install_input = {
                    "sourcePath": source,
                    "sourceUri": source,
                    "sourceType": "marketplace",
                }
                preview = preview_add(install_input)
                decisions = [
                    {
                        "skillId": candidate["skillId"],
                        "resolution": "update" if candidate["action"].startswith("blocked") else "create",
                    }
                    for candidate in preview["candidates"]
                ]
                installed = execute_add(install_input, decisions)
                skill_id = (
                    installed["skillIds"] + installed["updated"]
                )[0] if installed["skillIds"] or installed["updated"] else item["skillId"]
                results.append({
                    "itemId": item["itemId"],
                    "skillId": skill_id,
                    "success": True,
                    "error": None,
                })
            except Exception as error:
                results.append({
                    "itemId": item["itemId"],
                    "skillId": item["skillId"],
                    "success": False,
                    "error": str(error),
                })
        return {"items": results, "cancelled": False}
    if command == "cancel_marketplace_skill_batch":
        return False
    if command == "preview_github_repo_import":
        return github_preview(args["repoUrl"])
    if command == "import_github_repo_skills":
        return github_import(args["repoUrl"], args.get("selections", []), args.get("expectedCommit"))
    if command == "preview_delete_center_skill":
        return preview_delete([args["skillId"]])
    if command == "preview_delete_center_skills":
        return preview_delete(args.get("skillIds", []))
    if command in ("execute_delete_center_skill", "execute_delete_center_skills"):
        skill_ids = (
            [args["skillId"]]
            if command == "execute_delete_center_skill"
            else args.get("skillIds", [])
        )
        remove_linked = bool(args.get("removeLinked"))
        _, _, _, targets = inventory()
        for skill_id in skill_ids:
            skill_id = safe_id(skill_id)
            if remove_linked:
                for target in targets.values():
                    if target["skillId"] == skill_id:
                        remove_target_distribution(target)
            remove_path(center_path() / skill_id)
            STATE["sources"].pop(skill_id, None)
            for pack in STATE["packs"].values():
                pack["skillIds"] = [
                    item for item in pack.get("skillIds", [])
                    if item != skill_id
                ]
            for pack_id in list(STATE["packTargetClaims"]):
                filter_pack_target_claims(pack_id, lambda target_id: target_id.partition("::")[2] != skill_id)
        save_state()
        return None
    if command == "preview_distribute_skill":
        return distribution_preview(
            args.get("skillIds", []),
            args.get("targetAgents", []),
            args.get("requestedMode", "copy"),
        )
    if command == "execute_distribute_skill":
        return execute_distribution(args["preview"])
    if command == "scan_agent_inventory":
        agent_id = args["agentId"]
        data = next(item for item in unmanaged_inventory() if item["agentId"] == agent_id)
        return {
            "agentId": agent_id,
            "managed": data["managedCount"],
            "unmanaged": data["unmanagedCount"],
            "readOnly": data["readOnlyCount"],
        }
    if command == "list_unmanaged_v2":
        return inventory()[2]
    if command == "list_agent_skill_inventory_v2":
        return unmanaged_inventory()
    if command == "preview_adopt_agent_skill":
        return adopt_preview(args["agentId"], args["unmanagedId"])
    if command == "execute_adopt_agent_skill":
        return execute_adopt(
            args["agentId"],
            args["unmanagedId"],
            args["option"],
            args.get("renamedId"),
        )
    if command in ("execute_adopt_agent_skills", "takeover_center_agent_skills"):
        takeover = command == "takeover_center_agent_skills"
        items = args.get("items", [])
        if takeover:
            skills, _, _, targets = inventory()
            items = [
                {"agentId": args["agentId"], "unmanagedId": item}
                for item in args.get("unmanagedIds", [])
            ]
        results = []
        for item in items:
            try:
                if takeover:
                    skill_id = takeover_center_agent_skill(
                        item["agentId"], item["unmanagedId"], skills, targets,
                    )
                else:
                    skill_id = execute_adopt(
                        item["agentId"],
                        item["unmanagedId"],
                        item.get("option", "import_link"),
                        item.get("renamedId"),
                    )
                results.append({
                    "unmanagedId": item["unmanagedId"],
                    "skillId": skill_id or None,
                    "error": None,
                })
            except Exception as error:
                results.append({
                    "unmanagedId": item["unmanagedId"],
                    "skillId": None,
                    "error": str(error),
                })
        return {"items": results, "finalizationError": None}
    if command in ("delete_unmanaged_agent_skill", "delete_unmanaged_agent_skills"):
        requested_agent_id = args.get("agentId")
        ids = (
            [args["unmanagedId"]]
            if command == "delete_unmanaged_agent_skill"
            else args.get("unmanagedIds", [])
        )
        unmanaged_by_id = {
            item["id"]: item
            for item in inventory()[2]
        }
        failures = []
        deleted = 0
        for unmanaged_id in ids:
            try:
                item = unmanaged_by_id.get(unmanaged_id)
                if not item:
                    raise ValueError("Unmanaged Skill not found: " + unmanaged_id)
                remove_unmanaged_item(item, requested_agent_id)
                deleted += 1
            except Exception as error:
                failures.append({"unmanagedId": unmanaged_id, "error": str(error)})
        if command == "delete_unmanaged_agent_skill" and failures:
            raise ValueError(failures[0]["error"])
        return None if command == "delete_unmanaged_agent_skill" else {
            "deleted": deleted,
            "failures": failures,
        }
    if command in ("preview_sync_copy_target", "execute_sync_copy_target"):
        target_id = args["targetId"]
        skills, _, _, targets = inventory()
        target = targets.get(target_id)
        if not target:
            raise ValueError("Target not found: " + target_id)
        skill = skills[target["skillId"]]
        state = "ok" if target["sourceHash"] == target["currentHash"] else "copy_diverged"
        if command == "execute_sync_copy_target":
            action = args.get("action", "center_over_agent")
            source = pathlib.Path(skill["centerPath"])
            destination = pathlib.Path(target["targetPath"])
            if action in ("agent_over_center", "center_over_agent"):
                shared = target["agentId"] == SHARED_SKILLS_AGENT_ID
                if shared:
                    destination = prepare_shared_target_write(
                        destination, source, replace_center=(action == "agent_over_center"),
                    )
                if action == "agent_over_center":
                    source, destination = destination, source
                atomic_replace_skill_directory(source, destination, preserve_symlinks=not shared)
            skills, _, _, targets = inventory()
            target = targets[target_id]
            skill = skills[target["skillId"]]
            state = "ok"
        return {
            "targetId": target_id,
            "skillId": target["skillId"],
            "targetPath": target["targetPath"],
            "sourceHash": target["sourceHash"],
            "centerHash": skill["currentHash"],
            "copyHash": target["currentHash"],
            "state": state,
            "suggested": "none" if state == "ok" else "manual",
        }
    if command == "preview_copy_target_diff":
        target_id = args["targetId"]
        skills, _, _, targets = inventory()
        target = targets.get(target_id)
        if not target:
            raise ValueError("Target not found: " + target_id)
        center = pathlib.Path(skills[target["skillId"]]["centerPath"])
        copy = pathlib.Path(target["targetPath"])
        return {
            "targetId": target_id,
            "skillId": target["skillId"],
            "targetPath": target["targetPath"],
            "centerPath": skills[target["skillId"]]["centerPath"],
            "state": "ok" if target["sourceHash"] == target["currentHash"] else "copy_diverged",
            "files": copy_diff_files(center, copy),
        }
    if command in ("delete_skill_target_distribution", "delete_skill_target_distributions"):
        ids = (
            [args["targetId"]]
            if command == "delete_skill_target_distribution"
            else args.get("targetIds", [])
        )
        _, _, _, targets = inventory()
        failures = []
        deleted = 0
        claims_changed = False
        for target_id in ids:
            target = targets.get(target_id)
            if not target:
                failures.append({"targetId": target_id, "error": "Target not found"})
                continue
            try:
                claims_changed = remove_target_distribution(target) or claims_changed
                deleted += 1
            except Exception as error:
                failures.append({"targetId": target_id, "error": str(error)})
        if claims_changed:
            save_state()
        if command == "delete_skill_target_distribution":
            if failures:
                raise ValueError(failures[0]["error"])
            return None
        return {"deleted": deleted, "failures": failures}
    if command == "list_skill_packs_v2":
        return overview()["packs"]
    if command == "get_skill_pack_detail":
        return pack_detail(args["packId"])
    if command == "get_skill_pack_fingerprints":
        return skill_pack_fingerprints(args["skillIds"], args.get("includeSources", False))
    if command == "restore_skill_pack_member":
        return restore_pack_skill(args["manifest"], args["skillId"])
    if command == "execute_import_skill_pack_manifest":
        return import_reviewed_skill_pack(args["manifest"], args["reviewed"], args["acceptLocalCopies"])
    if command == "execute_upsert_skill_pack":
        return upsert_pack(args["pack"])
    if command == "preview_delete_skill_pack":
        detail = pack_detail(args["packId"])
        return {
            "packId": detail["id"],
            "packName": detail["name"],
            "appliedAgents": [
                item["agentId"] for item in detail["appliedAgents"]
            ],
            "affectedTargets": [],
            "removable": detail["id"] != "default",
            "warnings": [],
        }
    if command == "execute_delete_skill_pack":
        pack_id = safe_id(args["packId"])
        if pack_id == "default":
            raise ValueError("The default Skill Pack cannot be deleted")
        STATE["packs"].pop(pack_id, None)
        STATE["packTargetClaims"].pop(pack_id, None)
        save_state()
        return None
    if command == "preview_apply_skill_pack":
        detail = pack_detail(args["packId"])
        return distribution_preview(
            [item["skillId"] for item in detail["members"] if not item["missing"]],
            args.get("targetAgents", []),
            args.get("requestedMode", "copy"),
        )
    if command == "execute_apply_skill_pack":
        return apply_pack(
            args["packId"],
            args.get("targetAgents", []),
            args.get("requestedMode", "copy"),
            args.get("blockerDecisions", []),
        )
    if command == "execute_sync_skill_pack_to_agents":
        detail = pack_detail(args["packId"])
        target_agents = args.get("targetAgents") or [
            item["agentId"] for item in detail["appliedAgents"]
        ]
        apply_pack(args["packId"], target_agents, STATE["settings"]["defaultDistributeMode"])
        return {
            "packId": detail["id"],
            "packName": detail["name"],
            "revision": detail["revision"],
            "status": "synced",
            "agents": [
                {
                    "agentId": agent_id,
                    "displayName": agent_spec(agent_id)["displayName"],
                    "status": "synced",
                    "error": None,
                }
                for agent_id in target_agents
            ],
        }
    if command == "preview_remove_skill_pack_from_agent":
        pack_id = args["packId"]
        agent_id = args["agentId"]
        skills, agents, _, targets = inventory()
        detail = pack_detail(pack_id, skills, agents)
        affected = [
            affected_target(item, agents)
            for item in targets_claimed_by_pack(pack_id, targets)
            if item["agentId"] == agent_id
        ]
        removed = sum(item["claimCount"] == 1 for item in affected)
        return {
            "packId": detail["id"],
            "packName": detail["name"],
            "agentId": agent_id,
            "displayName": agent_spec(agent_id)["displayName"],
            "affectedTargets": affected,
            "willRemoveTargets": removed,
            "willPreserveTargets": len(affected) - removed,
        }
    if command == "execute_remove_skill_pack_from_agent":
        pack_id = args["packId"]
        agent_id = args["agentId"]
        skills, agents, _, targets = inventory()
        pack_detail(pack_id, skills, agents)
        agent_targets = [
            target for target in targets_claimed_by_pack(pack_id, targets)
            if target["agentId"] == agent_id
        ]
        removed = 0
        for target in agent_targets:
            if len(target["claims"]) == 1:
                remove_target_distribution(target)
                removed += 1
        if pack_id in STATE["packs"]:
            STATE["packs"][pack_id]["appliedAgents"] = [
                item for item in STATE["packs"][pack_id].get("appliedAgents", [])
                if item != agent_id
            ]
        filter_pack_target_claims(pack_id, lambda target_id: target_id.partition("::")[0] != agent_id)
        save_state()
        return {
            "packId": pack_id,
            "agentId": agent_id,
            "removedClaims": len(agent_targets),
            "removedTargets": removed,
            "preservedTargets": len(agent_targets) - removed,
        }
    if command == "preview_remove_skill_from_pack":
        pack_id = args["packId"]
        skill_id = args["skillId"]
        skills, agents, _, targets = inventory()
        detail = pack_detail(pack_id, skills, agents)
        member = next(
            (item for item in detail["members"] if item["skillId"] == args["skillId"]),
            None,
        )
        return {
            "packId": detail["id"],
            "packName": detail["name"],
            "skillId": args["skillId"],
            "skillName": member["skillName"] if member else args["skillId"],
            "affectedTargets": [
                affected_target(target, agents)
                for target in targets_claimed_by_pack(pack_id, targets)
                if target["skillId"] == skill_id
            ],
            "appliedAgentCount": len(detail["appliedAgents"]),
            "canKeepStandalone": True,
            "canRemoveTargets": True,
        }
    if command == "execute_remove_skill_from_pack":
        pack_id = args["packId"]
        skill_id = args["skillId"]
        pack = STATE["packs"].get(pack_id)
        if not pack:
            raise ValueError("Skill Pack not found")
        if args.get("alsoRemoveTargets"):
            _, _, _, targets = inventory()
            for target in targets_claimed_by_pack(pack_id, targets):
                if target["skillId"] == skill_id and len(target["claims"]) == 1:
                    remove_target_distribution(target)
        filter_pack_target_claims(pack_id, lambda target_id: target_id.partition("::")[2] != skill_id)
        pack["skillIds"] = [
            item for item in pack.get("skillIds", [])
            if item != skill_id
        ]
        pack["revision"] = int(pack.get("revision", 1)) + 1
        save_state()
        return None
    if command == "list_managed_agents_v2":
        return overview()["agents"]
    if command == "get_agent_detail_v2":
        return agent_detail(args["agentId"])
    if command == "refresh_agent_skill_view_v2":
        return agent_skill_view_snapshot(args["agentId"])
    if command == "read_agent_config_file_v2":
        return config_document(args["path"])
    if command == "write_agent_config_file_v2":
        return write_config(args["path"], args["content"], args["expectedRevision"])
    if command == "list_plugin_inventory_v2":
        return plugin_inventory(args["agentId"])
    if command == "set_plugin_enabled_v2":
        return set_plugin_enabled(args["agentId"], args["pluginId"], args["revision"], args["enabled"])
    if command == "get_plugin_detail_v2":
        return plugin_detail(args["agentId"], args["pluginId"])
    if command == "read_plugin_file_v2":
        return plugin_file(
            args["agentId"],
            args["pluginId"],
            args["relativePath"],
        )
    if command == "list_mcp_inventory_cmd":
        return mcp_inventory(args["agent"])
    if command == "validate_mcp_server_draft_cmd":
        server = args["server"]
        valid = bool(server.get("command") or server.get("url"))
        return {
            "valid": valid,
            "message": "MCP configuration is valid" if valid else "A command or URL is required",
            "warnings": [],
        }
    if command == "save_mcp_server_cmd":
        return write_mcp_inventory(
            args["agent"],
            args["revision"],
            server=args["server"],
            original_name=args.get("originalName"),
        )
    if command == "set_mcp_server_enabled_cmd":
        return write_mcp_inventory(
            args["agent"],
            args["revision"],
            toggle=(args["serverName"], bool(args["enabled"])),
        )
    if command == "delete_mcp_server_v2_cmd":
        return write_mcp_inventory(
            args["agent"], args["revision"], delete_name=args["serverName"]
        )
    if command == "test_mcp_server_connection_cmd":
        started = time.time()
        try:
            server = mcp_server(args["agent"], args["serverName"])
            with mcp_session(server) as (initialized, request):
                capabilities = advertised_mcp_capabilities(initialized)
                tools = request("tools/list", {}) if capabilities is None or "tools" in capabilities else {}
            server_info = initialized.get("serverInfo", {})
            return {
                "success": True,
                "category": "success",
                "message": "Remote MCP connection succeeded",
                "latencyMs": int((time.time() - started) * 1000),
                "protocolVersion": initialized.get("protocolVersion"),
                "serverName": server_info.get("name"),
                "serverVersion": server_info.get("version"),
                "toolCount": len((tools or {}).get("tools", [])),
            }
        except Exception as error:
            return {
                "success": False,
                "category": "connection_failed",
                "message": str(error),
                "latencyMs": int((time.time() - started) * 1000),
                "protocolVersion": None,
                "serverName": None,
                "serverVersion": None,
                "toolCount": None,
            }
    if command == "inspect_mcp_server_cmd":
        return inspect_mcp(
            args["agent"],
            args["serverName"],
            args["inspectionId"],
        )
    if command == "call_mcp_tool_cmd":
        started = time.time()
        result = mcp_request(
            args["agent"],
            args["serverName"],
            "tools/call",
            {
                "name": args["toolName"],
                "arguments": args.get("arguments", {}),
            },
        )
        return {
            "operationId": args["operationId"],
            "kind": "tool",
            "name": args["toolName"],
            "category": "tool_error" if (result or {}).get("isError") else "success",
            "durationMs": int((time.time() - started) * 1000),
            "result": result,
            "warnings": [],
        }
    if command == "get_mcp_prompt_cmd":
        started = time.time()
        result = mcp_request(
            args["agent"],
            args["serverName"],
            "prompts/get",
            {
                "name": args["promptName"],
                "arguments": args.get("arguments", {}),
            },
        )
        return {
            "operationId": args["operationId"],
            "kind": "prompt",
            "name": args["promptName"],
            "category": "success",
            "durationMs": int((time.time() - started) * 1000),
            "result": result,
            "warnings": [],
        }
    if command == "cancel_mcp_inspection_cmd":
        return cancel_mcp_request("inspection", args["inspectionId"])
    if command == "cancel_mcp_operation_cmd":
        return cancel_mcp_request("operation", args["operationId"])
    if command == "list_skill_projects_v2":
        return [
            project_summary(project)
            for project in STATE["projects"].values()
        ]
    if command == "add_skill_project_v2":
        root = project_root_path(args["rootPath"])
        project_id = hashlib.sha256(str(root).encode()).hexdigest()[:16]
        project = {
            "id": project_id,
            "name": root.name,
            "rootPath": str(root),
            "createdAt": now(),
            "updatedAt": now(),
        }
        STATE["projects"][project_id] = project
        save_state()
        return scan_project_data(project)
    if command == "remove_skill_project_v2":
        STATE["projects"].pop(args["projectId"], None)
        save_state()
        return None
    if command in ("get_skill_project_detail_v2", "scan_skill_project_v2"):
        project = STATE["projects"].get(args["projectId"])
        if not project:
            raise ValueError("Project not found")
        detail = scan_project_data(project)
        project["updatedAt"] = detail["updatedAt"]
        project["lastScannedAt"] = detail["lastScannedAt"]
        save_state()
        return detail
    if command == "install_center_skills_to_project_v2":
        return install_to_project(
            args["projectId"],
            args["agentId"],
            args.get("skillIds", []),
            args.get("requestedMode", "copy"),
        )
    if command == "install_skill_pack_to_project_v2":
        detail = pack_detail(args["packId"])
        return install_to_project(
            args["projectId"],
            args["agentId"],
            [item["skillId"] for item in detail["members"] if not item["missing"]],
            args.get("requestedMode", "copy"),
        )
    if command in ("run_skill_manager_diagnosis", "list_diagnosis_issues"):
        return diagnosis()
    if command == "preview_fix_diagnosis_issue":
        issue = next(
            (item for item in diagnosis() if item["issueType"] == args["issueType"]
             and item.get("entityId") == args["entityId"]),
            None,
        )
        return {"issue": issue, "destructive": bool(issue)}
    if command == "execute_fix_diagnosis_issue":
        if args["issueType"] == "copy_diverged":
            target_id = args["entityId"]
            skills, _, _, targets = inventory()
            target = targets.get(target_id)
            if not target:
                raise ValueError("Target not found")
            source = pathlib.Path(skills[target["skillId"]]["centerPath"])
            destination = pathlib.Path(target["targetPath"])
            shared = target["agentId"] == SHARED_SKILLS_AGENT_ID
            if shared:
                destination = prepare_shared_target_write(destination, source)
            atomic_replace_skill_directory(source, destination, preserve_symlinks=not shared)
        return None
    if command == "execute_safe_fixes":
        return 0
    if command == "preview_move_direct_skill_to_pack":
        return move_direct_preview(args["targetId"], args["packId"])
    if command == "execute_move_direct_skill_to_pack":
        preview = move_direct_preview(args["targetId"], args["packId"])
        pack_id = preview["packId"]
        if preview["willAddToPack"]:
            pack = STATE["packs"][pack_id]
            pack["skillIds"].append(preview["skillId"])
            pack["revision"] = int(pack.get("revision", 1)) + 1
            pack["updatedAt"] = now()
        add_pack_target_claims(pack_id, [preview["targetId"]])
        save_state()
        return preview
    if command == "skill_manager_export_snapshot":
        snapshot = center_path() / "agentbro-skills.snapshot.json"
        write_json_atomically(snapshot, overview())
        return str(snapshot)
    if command in ("open_skill_path", "reveal_skill_path", "open_system_path"):
        path = safe_home_path(args["path"], allow_missing=False)
        return {
            "path": str(path),
            "parentPath": str(path.parent),
            "name": path.name,
            "isDirectory": path.is_dir(),
        }
    if command in ("agent_list", "agent_refresh"):
        return agent_programs()
    if command in ("agent_install", "agent_update", "agent_uninstall"):
        return run_agent_package_operation(
            args["agentId"],
            command[len("agent_"):],
        )
    if command in ("agent_open_download", "agent_open_app"):
        return None
    if command == "add_custom_agent":
        config = args["config"]
        name = config["displayName"].strip()
        explicit_id = config.get("id")
        generated_unicode_id = not explicit_id and not name.isascii()
        if generated_unicode_id:
            agent_id = "custom-unicode-" + hashlib.sha256(name.encode("utf-8")).hexdigest()
        else:
            slug = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")
            agent_id = safe_id(explicit_id or "custom-" + (slug or uuid.uuid4().hex[:10]))
        if (
            agent_id == SHARED_SKILLS_AGENT_ID
            or any(spec["id"] == agent_id for spec in AGENTS)
            or agent_id in STATE["customAgents"]
        ):
            raise ValueError("Agent already exists: " + agent_id)
        if generated_unicode_id:
            for existing_id, agent in STATE["customAgents"].items():
                if agent.get("displayName") == name:
                    raise ValueError("Agent already exists: " + existing_id)
        value = custom_agent_program(agent_id, config)
        STATE["customAgents"][agent_id] = value
        save_state()
        return value
    if command == "update_custom_agent":
        agent_id = args["agentId"]
        existing = STATE["customAgents"].get(agent_id)
        if existing is None:
            raise ValueError("Custom Agent not found")
        config = dict(args["config"])
        for field in ("category", "settingsFile", "mcpConfig", "pluginDir"):
            if field not in config:
                config[field] = existing.get(field)
        value = custom_agent_program(agent_id, config)
        STATE["customAgents"][agent_id] = value
        save_state()
        return value
    if command == "remove_custom_agent":
        STATE["customAgents"].pop(args["agentId"], None)
        save_state()
        return None
    if command in (
        "get_skill_explanation_cmd",
        "generate_skill_explanation_cmd",
    ):
        raise ValueError("This operation is not available through the remote SSH runtime yet")
    raise ValueError("Unsupported remote Skill Manager command: " + command)


if COMMAND == "inspect_mcp_server_cmd":
    supervise_mcp_request("inspection", ARGS["inspectionId"])
elif COMMAND in ("call_mcp_tool_cmd", "get_mcp_prompt_cmd"):
    supervise_mcp_request("operation", ARGS["operationId"])

try:
    RESPONSE = json_safe(dispatch())
    print(MARKER + json.dumps(RESPONSE, ensure_ascii=True))
except Exception as error:
    raise RuntimeError(str(error)) from error
finally:
    if STATE_LOCK is not None:
        fcntl.flock(STATE_LOCK.fileno(), fcntl.LOCK_UN)
        STATE_LOCK.close()
