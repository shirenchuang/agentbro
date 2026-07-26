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
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request

MARKER = "__AGENTBRO_REMOTE_SKILL_MANAGER__"
REQUEST = json.loads(base64.b64decode("__AGENTBRO_REQUEST_B64__").decode())
COMMAND = REQUEST.get("command", "")
ARGS = REQUEST.get("args") or {}
HOME = pathlib.Path.home().resolve()
STATE_PATH = HOME / ".agentbro" / "skill-manager-remote.json"
FIXED_CENTER_PATH = HOME / ".agentbro" / "skills"
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
    "execute_upsert_skill_pack",
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
        "projects": {},
        "customAgents": {},
    }


def load_state():
    state = default_state()
    if STATE_PATH.is_file():
        try:
            saved = json.loads(STATE_PATH.read_text())
            for key in ("settings", "sources", "packs", "projects", "customAgents"):
                if isinstance(saved.get(key), dict):
                    state[key].update(saved[key])
        except Exception:
            pass
    return state


STATE = load_state()
CONFIGURED_CENTER_PATH = str(STATE["settings"].get("centerPath") or FIXED_CENTER_PATH)
STATE["settings"]["centerPath"] = str(FIXED_CENTER_PATH)


def save_state():
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    temp_path = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            dir=STATE_PATH.parent,
            prefix=f".{STATE_PATH.name}.",
            suffix=".tmp",
            delete=False,
        ) as temp:
            json.dump(STATE, temp, ensure_ascii=False, indent=2)
            temp.flush()
            os.fsync(temp.fileno())
            temp_path = pathlib.Path(temp.name)
        temp_path.replace(STATE_PATH)
    finally:
        if temp_path is not None:
            unlink_if_exists(temp_path)


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


def agent_spec(agent_id):
    return next((item for item in AGENTS if item["id"] == agent_id), None)


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
    return HOME / spec["config"]


def agent_mcp_path(agent_id):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + str(agent_id))
    return agent_config_path(agent_id) if agent_id == "openclaw" else HOME / spec["mcp"]


def agent_plugin_path(agent_id):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + str(agent_id))
    if agent_id == "openclaw":
        return openclaw_state_path() / "extensions"
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


def copy_or_link(source, target, mode):
    target.parent.mkdir(parents=True, exist_ok=True)
    if mode == "link":
        target.symlink_to(source, target_is_directory=True)
        return "link"
    shutil.copytree(source, target, symlinks=True)
    return "copy"


def applied_pack_ids(agent_id):
    return [
        pack_id for pack_id, pack in STATE["packs"].items()
        if agent_id in pack.get("appliedAgents", [])
    ]


def pack_claims(agent_id, skill_id):
    claims = []
    stamp = now()
    for pack_id in applied_pack_ids(agent_id):
        pack = STATE["packs"].get(pack_id, {})
        if skill_id in pack.get("skillIds", []):
            claims.append({
                "id": "pack::" + pack_id + "::" + agent_id + "::" + skill_id,
                "claimType": "pack",
                "packId": pack_id,
                "packName": pack.get("name", pack_id),
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
    for spec in AGENTS:
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
    return skills, agents, unmanaged, targets


def pack_detail(pack_id, skills=None, agents=None):
    if skills is None or agents is None:
        skills, agents, _, _ = inventory()
    if pack_id == "default":
        data = {
            "id": "default",
            "name": "全量技能包",
            "description": "中心库全部 Skills。应用时按当前中心库全量分发。",
            "tags": [],
            "skillIds": list(skills),
            "appliedAgents": [],
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
    for agent_id in data.get("appliedAgents", []):
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


def overview():
    skills, agents, unmanaged, targets = inventory()
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


def unmanaged_inventory():
    skills, agents, unmanaged, targets = inventory()
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
    return result


def find_unmanaged(unmanaged_id):
    _, _, unmanaged, _ = inventory()
    item = next((item for item in unmanaged if item["id"] == unmanaged_id), None)
    if not item:
        raise ValueError("Unmanaged Skill not found: " + unmanaged_id)
    return item


def adopt_preview(agent_id, unmanaged_id):
    item = find_unmanaged(unmanaged_id)
    if item["agentId"] != agent_id:
        raise ValueError("Unmanaged Skill does not belong to this Agent")
    skill_id = safe_id(item["inferredSkillId"])
    target = center_path() / skill_id
    same_id = target.exists()
    return {
        "agentId": agent_id,
        "unmanagedId": unmanaged_id,
        "skillPath": item["path"],
        "inferredSkillId": skill_id,
        "hash": item["hash"],
        "centerHasSameId": same_id,
        "canQuickAdopt": not same_id,
        "options": [
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
        ],
    }


def execute_adopt(agent_id, unmanaged_id, option, renamed_id=None):
    preview = adopt_preview(agent_id, unmanaged_id)
    if option == "skip":
        return ""
    source = pathlib.Path(preview["skillPath"])
    skill_id = safe_id(renamed_id if option == "rename" else preview["inferredSkillId"])
    target = center_path() / skill_id
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
        return "https://github.com/" + parts[0] + "/" + parts[1] + ".git", "/".join(parts[2:])
    if source.startswith("https://skills.sh/"):
        spec = source[len("https://skills.sh/"):].strip("/")
        parts = [item for item in spec.split("/") if item]
        if len(parts) < 3:
            raise ValueError("skills.sh source must be owner/repository/skill-id")
        return "https://github.com/" + parts[0] + "/" + parts[1] + ".git", "/".join(parts[2:])
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
                "/".join(requested) or None,
            )
    if source.startswith("https://github.com/"):
        parsed = urllib.parse.urlparse(source)
        parts = [item for item in parsed.path.strip("/").split("/") if item]
        if len(parts) >= 5 and parts[2] in ("tree", "blob"):
            requested = parts[4:]
            if requested and requested[-1] == "SKILL.md":
                requested = requested[:-1]
            return (
                "https://github.com/" + parts[0] + "/" + parts[1] + ".git",
                "/".join(requested) or None,
            )
    if source.startswith(("https://github.com/", "https://gitlab.com/", "git@github.com:", "git@gitlab.com:")):
        return source, None
    return None, None


def source_candidates(input_value):
    source_path = str(input_value.get("sourcePath") or "")
    repository, requested = normalize_repository(
        input_value.get("sourceUri") or source_path
    )
    temporary = None
    if repository:
        temporary = tempfile.TemporaryDirectory(prefix="agentbro-remote-skill-")
        root = pathlib.Path(temporary.name) / "repo"
        subprocess.run(
            ["git", "clone", "--depth", "1", repository, str(root)],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
    else:
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
    if requested:
        requested_parts = tuple(item for item in requested.split("/") if item)
        candidates = [
            item for item in candidates
            if item.name == requested_parts[-1]
            or item.relative_to(root).parts[-len(requested_parts):] == requested_parts
        ]
    return root, candidates, temporary


def preview_add(input_value):
    root, candidates, temporary = source_candidates(input_value)
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
    root, candidates, temporary = source_candidates(input_value)
    try:
        decision_map = {item["skillId"]: item for item in decisions}
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
                "sourceUri": input_value.get("sourceUri"),
                "sourceRef": None,
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
    pack_id = safe_id(pack["id"])
    if pack_id == "default":
        raise ValueError("The default Skill Pack cannot be edited")
    existing = STATE["packs"].get(pack_id, {})
    stamp = now()
    STATE["packs"][pack_id] = {
        "id": pack_id,
        "name": str(pack.get("name") or pack_id),
        "description": str(pack.get("description") or ""),
        "tags": [str(item) for item in pack.get("tags", [])],
        "skillIds": [safe_id(item) for item in pack.get("skillIds", [])],
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


def agent_detail(agent_id):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + agent_id)
    skills, agents, _, targets = inventory()
    agent = next(item for item in agents if item["id"] == agent_id)
    applied = []
    for pack_id in applied_pack_ids(agent_id):
        detail = pack_detail(pack_id, skills, agents)
        applied.extend(detail["appliedAgents"])
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
        "agentDir": str(agent_skills_path(agent_id).parent),
        "skills": [item for item in targets.values() if item["agentId"] == agent_id],
        "appliedPacks": applied,
        "availablePacks": overview()["packs"],
        "mcpServers": [],
        "plugins": [],
        "health": [],
    }


def config_document(path):
    path = safe_home_path(path)
    content = path.read_text(errors="replace") if path.is_file() else ""
    revision = hashlib.sha256(content.encode()).hexdigest()
    return {"path": str(path), "content": content, "revision": revision}


def write_config(path, content, expected_revision):
    current = config_document(path)
    if current["revision"] != expected_revision:
        raise ValueError("The remote configuration changed after it was opened")
    target = safe_home_path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content)
    return config_document(target)


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
    root = project_root_path(project["rootPath"])
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + agent_id)
    target_root = root / spec["skills"]
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


def clone_repository(repo_url):
    repository, requested = normalize_repository(repo_url)
    if not repository:
        raise ValueError("Only GitHub, GitLab, and skills.sh repositories are supported")
    temporary = tempfile.TemporaryDirectory(prefix="agentbro-remote-repo-")
    root = pathlib.Path(temporary.name) / "repo"
    subprocess.run(
        ["git", "clone", "--depth", "1", repository, str(root)],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return repository, requested, root, temporary


def repository_info(repo_url):
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
        "branch": "HEAD",
        "normalizedUrl": clean,
    }


def github_preview(repo_url):
    repository, requested, root, temporary = clone_repository(repo_url)
    try:
        candidates = [root] if (root / "SKILL.md").is_file() else [
            item.parent for item in root.rglob("SKILL.md")
            if len(item.relative_to(root).parts) <= 7
        ]
        if requested:
            parts = tuple(item for item in requested.split("/") if item)
            candidates = [
                item for item in candidates
                if item.name == parts[-1]
                or item.relative_to(root).parts[-len(parts):] == parts
            ]
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
        return {"repo": repository_info(repository), "skills": skills}
    finally:
        temporary.cleanup()


def github_import(repo_url, selections):
    repository, requested, root, temporary = clone_repository(repo_url)
    try:
        imported = []
        skipped = []
        for selection in selections:
            relative = pathlib.Path(str(selection.get("sourcePath") or ""))
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError("Invalid repository Skill path")
            source = root / relative
            if not source.is_dir() or not (source / "SKILL.md").is_file():
                raise ValueError("Skill path not found in repository: " + str(relative))
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
                "sourceUri": repository,
                "sourceRef": str(relative),
                "importedFromAgent": None,
                "importedFromPath": None,
                "installedVia": "remote_git",
                "createdAt": now(),
                "updatedAt": now(),
            }
        save_state()
        return {
            "repo": repository_info(repository),
            "importedSkills": imported,
            "skippedSkills": skipped,
        }
    finally:
        temporary.cleanup()


def mcp_inventory(agent_id):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + agent_id)
    path = agent_mcp_path(agent_id)
    content = path.read_text(errors="replace") if path.is_file() else ""
    revision = hashlib.sha256(content.encode()).hexdigest()
    editable = path.suffix == ".json"
    data = {}
    if content and editable:
        try:
            data = json.loads(content)
        except Exception:
            data = {}
    raw_servers = data.get("mcpServers", {}) if isinstance(data, dict) else {}
    disabled = set(data.get("disabledMcpServers", [])) if isinstance(data, dict) else set()
    servers = []
    for name, value in raw_servers.items():
        if not isinstance(value, dict):
            continue
        transport = value.get("type")
        if not transport:
            transport = "http" if value.get("url") else "stdio"
        env = [
            {
                "key": str(key),
                "value": str(item),
                "secret": bool(re.search(r"(token|secret|password|key)", str(key), re.I)),
                "configured": True,
            }
            for key, item in (value.get("env") or {}).items()
        ]
        headers = [
            {
                "key": str(key),
                "value": str(item),
                "secret": bool(re.search(r"(authorization|token|secret|key)", str(key), re.I)),
                "configured": True,
            }
            for key, item in (value.get("headers") or {}).items()
        ]
        servers.append({
            "name": name,
            "transport": transport,
            "command": value.get("command"),
            "args": value.get("args", []),
            "env": env,
            "cwd": value.get("cwd"),
            "url": value.get("url"),
            "headers": headers,
            "enabled": name not in disabled,
            "disabledByAgentbro": name in disabled,
            "valid": bool(value.get("command") or value.get("url")),
            "message": "",
            "warnings": [],
            "configPath": str(path),
            "editable": editable,
            "sourceKind": "remote_config",
        })
    return {
        "agentId": agent_id,
        "configPath": str(path),
        "revision": revision,
        "capabilities": {
            "editable": editable,
            "supportsStdio": True,
            "supportsHttp": True,
            "supportsSse": True,
            "supportsNativeToggle": editable,
        },
        "servers": servers,
    }


def write_mcp_inventory(agent_id, server=None, original_name=None, delete_name=None, toggle=None):
    current = mcp_inventory(agent_id)
    if not current["capabilities"]["editable"]:
        raise ValueError("This remote Agent MCP format is read-only")
    path = pathlib.Path(current["configPath"])
    data = {}
    if path.is_file():
        try:
            data = json.loads(path.read_text())
        except Exception:
            data = {}
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
        if original_name and original_name != name:
            servers.pop(original_name, None)
            disabled.discard(original_name)
        value = {"type": server.get("transport", "stdio")}
        for key in ("command", "args", "cwd", "url"):
            if server.get(key) not in (None, "", []):
                value[key] = server[key]
        if server.get("env"):
            value["env"] = {
                item["key"]: item.get("value", "")
                for item in server["env"]
                if item.get("key")
            }
        if server.get("headers"):
            value["headers"] = {
                item["key"]: item.get("value", "")
                for item in server["headers"]
                if item.get("key")
            }
        servers[name] = value
    data["disabledMcpServers"] = sorted(disabled)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2))
    return mcp_inventory(agent_id)


def mcp_server(agent_id, server_name):
    inventory_value = mcp_inventory(agent_id)
    server = next(
        (item for item in inventory_value["servers"] if item["name"] == server_name),
        None,
    )
    if not server:
        raise ValueError("MCP server not found: " + server_name)
    if not server["enabled"]:
        raise ValueError("MCP server is disabled")
    return server


def read_json_line(stream, timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        ready, _, _ = select.select([stream], [], [], max(0.1, deadline - time.time()))
        if not ready:
            continue
        line = stream.readline()
        if not line:
            raise ValueError("MCP process exited before responding")
        try:
            value = json.loads(line)
        except Exception:
            continue
        if isinstance(value, dict):
            return value
    raise TimeoutError("MCP server response timed out")


def stdio_mcp_request(server, method, params=None):
    command = server.get("command")
    if not command:
        raise ValueError("MCP stdio server has no command")
    environment = os.environ.copy()
    for item in server.get("env", []):
        if item.get("key") and item.get("value") is not None:
            environment[item["key"]] = item["value"]
    process = subprocess.Popen(
        [command] + [str(item) for item in server.get("args", [])],
        cwd=server.get("cwd") or None,
        env=environment,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )

    def send(value):
        process.stdin.write(json.dumps(value, separators=(",", ":")) + "\n")
        process.stdin.flush()

    def request(request_id, request_method, request_params=None):
        payload = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": request_method,
        }
        if request_params is not None:
            payload["params"] = request_params
        send(payload)
        while True:
            response = read_json_line(process.stdout, 15)
            if response.get("id") == request_id:
                if response.get("error"):
                    raise ValueError(json.dumps(response["error"], ensure_ascii=False))
                return response.get("result")

    try:
        initialized = request(1, "initialize", {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "AgentBro", "version": "remote"},
        })
        send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        result = request(2, method, params)
        return initialized, result
    finally:
        process.terminate()
        try:
            process.wait(timeout=2)
        except Exception:
            process.kill()


def http_mcp_post(url, headers, payload, session_id=None):
    request_headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
    for item in headers:
        if item.get("key") and item.get("value") is not None:
            request_headers[item["key"]] = item["value"]
    if session_id:
        request_headers["Mcp-Session-Id"] = session_id
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers=request_headers,
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=20) as response:
        raw = response.read().decode(errors="replace")
        session = response.headers.get("Mcp-Session-Id") or session_id
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


def http_mcp_request(server, method, params=None):
    url = server.get("url")
    if not url:
        raise ValueError("Remote MCP server has no URL")
    initialized, session_id = http_mcp_post(
        url,
        server.get("headers", []),
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
    result, _ = http_mcp_post(
        url,
        server.get("headers", []),
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": method,
            "params": params or {},
        },
        session_id,
    )
    return initialized, result


def mcp_request(agent_id, server_name, method, params=None):
    server = mcp_server(agent_id, server_name)
    if server["transport"] == "stdio":
        initialized, result = stdio_mcp_request(server, method, params)
    else:
        initialized, result = http_mcp_request(server, method, params)
    return server, initialized, result


def inspect_mcp(agent_id, server_name, inspection_id):
    started = time.time()
    server, initialized, tools = mcp_request(agent_id, server_name, "tools/list", {})
    warnings = []
    try:
        _, _, resources = mcp_request(agent_id, server_name, "resources/list", {})
    except Exception as error:
        resources = {"resources": []}
        warnings.append(str(error))
    try:
        _, _, prompts = mcp_request(agent_id, server_name, "prompts/list", {})
    except Exception as error:
        prompts = {"prompts": []}
        warnings.append(str(error))
    server_info = (initialized or {}).get("serverInfo", {})
    return {
        "inspectionId": inspection_id,
        "status": "partial" if warnings else "connected",
        "category": "success",
        "summary": "Remote MCP inspection completed",
        "inspectedAtMs": int(time.time() * 1000),
        "durationMs": int((time.time() - started) * 1000),
        "protocolVersion": (initialized or {}).get("protocolVersion"),
        "serverName": server_info.get("name"),
        "serverVersion": server_info.get("version"),
        "transport": server["transport"],
        "capabilities": {
            "tools": bool((tools or {}).get("tools")),
            "resources": bool((resources or {}).get("resources")),
            "prompts": bool((prompts or {}).get("prompts")),
            "logging": False,
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
            for item in (tools or {}).get("tools", [])
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
            for item in (resources or {}).get("resources", [])
        ],
        "prompts": [
            {
                "name": item.get("name", ""),
                "title": item.get("title"),
                "description": item.get("description"),
                "arguments": item.get("arguments", []),
            }
            for item in (prompts or {}).get("prompts", [])
        ],
        "steps": [],
        "warnings": warnings,
        "suggestions": [],
    }


def plugin_inventory(agent_id):
    spec = agent_spec(agent_id)
    if not spec:
        raise ValueError("Unknown Agent: " + agent_id)
    root = agent_plugin_path(agent_id)
    plugins = []
    if root.is_dir():
        for path in sorted(root.iterdir(), key=lambda item: item.name.lower()):
            if not path.is_dir():
                continue
            manifest = path / "package.json"
            metadata = {}
            if manifest.is_file():
                try:
                    metadata = json.loads(manifest.read_text())
                except Exception:
                    metadata = {}
            plugins.append({
                "id": path.name,
                "name": metadata.get("name", path.name),
                "version": metadata.get("version"),
                "enabled": not (path / ".disabled").exists(),
                "source": str(path),
            })
    revision = hashlib.sha256(
        json.dumps(plugins, sort_keys=True).encode()
    ).hexdigest()
    return {
        "agentId": agent_id,
        "configPath": str(root),
        "revision": revision,
        "capabilities": {"editable": True, "requiresNewSession": True},
        "plugins": plugins,
    }


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
    root = safe_home_path(plugin["source"], allow_missing=False)
    manifest = root / "package.json"
    metadata = {}
    if manifest.is_file():
        try:
            metadata = json.loads(manifest.read_text())
        except Exception:
            metadata = {}
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
    path = root / relative
    if not path.is_file():
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
    detail = pack_detail(pack_id, skills, agents)
    skill = skills[target["skillId"]]
    member_ids = [item["skillId"] for item in detail["members"] if not item["missing"]]
    already_member = target["skillId"] in member_ids
    already_applied = target["agentId"] in [
        item["agentId"] for item in detail["appliedAgents"]
    ]
    remaining = [item for item in member_ids if item != target["skillId"]]
    distribution = distribution_preview(
        remaining,
        [target["agentId"]],
        target["actualMode"],
    )
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
        "willAddToPack": not already_member,
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
    result.extend(STATE["customAgents"].values())
    return result


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
        return github_import(args["repoUrl"], args.get("selections", []))
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
                        remove_path(pathlib.Path(target["targetPath"]))
            remove_path(center_path() / skill_id)
            STATE["sources"].pop(skill_id, None)
            for pack in STATE["packs"].values():
                pack["skillIds"] = [
                    item for item in pack.get("skillIds", [])
                    if item != skill_id
                ]
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
        items = args.get("items", [])
        if command == "takeover_center_agent_skills":
            items = [
                {
                    "agentId": args["agentId"],
                    "unmanagedId": item,
                    "option": "overwrite_center",
                    "renamedId": None,
                }
                for item in args.get("unmanagedIds", [])
            ]
        results = []
        for item in items:
            try:
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
        ids = (
            [args["unmanagedId"]]
            if command == "delete_unmanaged_agent_skill"
            else args.get("unmanagedIds", [])
        )
        failures = []
        deleted = 0
        for unmanaged_id in ids:
            try:
                item = find_unmanaged(unmanaged_id)
                remove_path(pathlib.Path(item["path"]))
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
            if action == "agent_over_center":
                remove_path(source)
                shutil.copytree(destination, source, symlinks=True)
            elif action == "center_over_agent":
                remove_path(destination)
                shutil.copytree(source, destination, symlinks=True)
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
        for target_id in ids:
            target = targets.get(target_id)
            if not target:
                failures.append({"targetId": target_id, "error": "Target not found"})
                continue
            remove_path(pathlib.Path(target["targetPath"]))
            deleted += 1
        if command == "delete_skill_target_distribution":
            if failures:
                raise ValueError(failures[0]["error"])
            return None
        return {"deleted": deleted, "failures": failures}
    if command == "list_skill_packs_v2":
        return overview()["packs"]
    if command == "get_skill_pack_detail":
        return pack_detail(args["packId"])
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
        detail = pack_detail(args["packId"])
        agent_id = args["agentId"]
        _, agents, _, targets = inventory()
        affected = [
            affected_target(item, agents)
            for item in targets.values()
            if item["agentId"] == agent_id
            and any(member["skillId"] == item["skillId"] for member in detail["members"])
        ]
        return {
            "packId": detail["id"],
            "packName": detail["name"],
            "agentId": agent_id,
            "displayName": agent_spec(agent_id)["displayName"],
            "affectedTargets": affected,
            "willRemoveTargets": len(affected),
            "willPreserveTargets": 0,
        }
    if command == "execute_remove_skill_pack_from_agent":
        pack_id = args["packId"]
        agent_id = args["agentId"]
        detail = pack_detail(pack_id)
        removed = 0
        for member in detail["members"]:
            target = agent_skills_path(agent_id) / member["skillId"]
            if target.exists() or target.is_symlink():
                remove_path(target)
                removed += 1
        if pack_id in STATE["packs"]:
            STATE["packs"][pack_id]["appliedAgents"] = [
                item for item in STATE["packs"][pack_id].get("appliedAgents", [])
                if item != agent_id
            ]
            save_state()
        return {
            "packId": pack_id,
            "agentId": agent_id,
            "removedClaims": removed,
            "removedTargets": removed,
            "preservedTargets": 0,
        }
    if command == "preview_remove_skill_from_pack":
        detail = pack_detail(args["packId"])
        member = next(
            (item for item in detail["members"] if item["skillId"] == args["skillId"]),
            None,
        )
        return {
            "packId": detail["id"],
            "packName": detail["name"],
            "skillId": args["skillId"],
            "skillName": member["skillName"] if member else args["skillId"],
            "affectedTargets": [],
            "appliedAgentCount": len(detail["appliedAgents"]),
            "canKeepStandalone": True,
            "canRemoveTargets": True,
        }
    if command == "execute_remove_skill_from_pack":
        pack = STATE["packs"].get(args["packId"])
        if not pack:
            raise ValueError("Skill Pack not found")
        pack["skillIds"] = [
            item for item in pack.get("skillIds", [])
            if item != args["skillId"]
        ]
        pack["revision"] = int(pack.get("revision", 1)) + 1
        if args.get("alsoRemoveTargets"):
            for agent_id in pack.get("appliedAgents", []):
                remove_path(agent_skills_path(agent_id) / args["skillId"])
        save_state()
        return None
    if command == "list_managed_agents_v2":
        return overview()["agents"]
    if command == "get_agent_detail_v2":
        return agent_detail(args["agentId"])
    if command == "read_agent_config_file_v2":
        return config_document(args["path"])
    if command == "write_agent_config_file_v2":
        return write_config(args["path"], args["content"], args["expectedRevision"])
    if command == "list_plugin_inventory_v2":
        return plugin_inventory(args["agentId"])
    if command == "set_plugin_enabled_v2":
        inventory_value = plugin_inventory(args["agentId"])
        plugin = next(
            (item for item in inventory_value["plugins"] if item["id"] == args["pluginId"]),
            None,
        )
        if not plugin:
            raise ValueError("Plugin not found")
        marker = pathlib.Path(plugin["source"]) / ".disabled"
        if args["enabled"]:
            unlink_if_exists(marker)
        else:
            marker.touch()
        return plugin_inventory(args["agentId"])
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
            server=args["server"],
            original_name=args.get("originalName"),
        )
    if command == "set_mcp_server_enabled_cmd":
        return write_mcp_inventory(
            args["agent"],
            toggle=(args["serverName"], bool(args["enabled"])),
        )
    if command == "delete_mcp_server_v2_cmd":
        return write_mcp_inventory(args["agent"], delete_name=args["serverName"])
    if command == "test_mcp_server_connection_cmd":
        started = time.time()
        try:
            _, initialized, tools = mcp_request(
                args["agent"],
                args["serverName"],
                "tools/list",
                {},
            )
            server_info = (initialized or {}).get("serverInfo", {})
            return {
                "success": True,
                "category": "success",
                "message": "Remote MCP connection succeeded",
                "latencyMs": int((time.time() - started) * 1000),
                "protocolVersion": (initialized or {}).get("protocolVersion"),
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
        _, _, result = mcp_request(
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
        _, _, result = mcp_request(
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
    if command in ("cancel_mcp_inspection_cmd", "cancel_mcp_operation_cmd"):
        return None
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
            remove_path(destination)
            shutil.copytree(source, destination, symlinks=True)
        return None
    if command == "execute_safe_fixes":
        return 0
    if command == "preview_move_direct_skill_to_pack":
        return move_direct_preview(args["targetId"], args["packId"])
    if command == "execute_move_direct_skill_to_pack":
        preview = move_direct_preview(args["targetId"], args["packId"])
        pack = STATE["packs"].get(args["packId"])
        if not pack:
            raise ValueError("Skill Pack not found")
        if preview["willAddToPack"]:
            pack["skillIds"].append(preview["skillId"])
            pack["revision"] = int(pack.get("revision", 1)) + 1
        if preview["agentId"] not in pack.get("appliedAgents", []):
            pack.setdefault("appliedAgents", []).append(preview["agentId"])
        save_state()
        if preview["distribution"]["skillIds"]:
            distribution = preview["distribution"]
            distribution["blockerDecisions"] = args.get("blockerDecisions", [])
            execute_distribution(distribution)
        return move_direct_preview(args["targetId"], args["packId"])
    if command == "skill_manager_export_snapshot":
        snapshot = center_path() / "agentbro-skills.snapshot.json"
        snapshot.write_text(json.dumps(overview(), ensure_ascii=False, indent=2))
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
        agent_id = safe_id(
            config.get("id")
            or "custom-" + re.sub(r"[^a-z0-9]+", "-", config["displayName"].lower()).strip("-")
        )
        value = {
            "id": agent_id,
            "displayName": config["displayName"],
            "icon": config.get("iconName") or "custom",
            "kind": "cli",
            "status": "installed",
            "packageManager": "custom",
            "packageName": None,
            "installedVersion": None,
            "latestVersion": None,
            "binaryPath": None,
            "configDir": config.get("configDir") or config["globalSkillsDir"],
            "appPath": None,
            "downloadUrl": None,
            "installCommand": None,
            "updateCommand": None,
            "uninstallCommand": None,
            "hooksInstalled": False,
            "skillsDir": config["globalSkillsDir"],
            "isCustom": True,
        }
        STATE["customAgents"][agent_id] = value
        save_state()
        return value
    if command == "update_custom_agent":
        agent_id = args["agentId"]
        if agent_id not in STATE["customAgents"]:
            raise ValueError("Custom Agent not found")
        config = args["config"]
        value = STATE["customAgents"][agent_id]
        value.update({
            "displayName": config["displayName"],
            "icon": config.get("iconName") or "custom",
            "configDir": config.get("configDir") or config["globalSkillsDir"],
            "skillsDir": config["globalSkillsDir"],
        })
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


try:
    RESPONSE = json_safe(dispatch())
    print(MARKER + json.dumps(RESPONSE, ensure_ascii=True))
except Exception as error:
    raise RuntimeError(str(error)) from error
finally:
    if STATE_LOCK is not None:
        fcntl.flock(STATE_LOCK.fileno(), fcntl.LOCK_UN)
        STATE_LOCK.close()
