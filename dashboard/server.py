#!/usr/bin/env python3
"""Small read-only/control dashboard for the XRY LightFlow worker."""
from __future__ import annotations

import datetime as dt
import json
import os
import subprocess
import tempfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path("/srv")
MATERIAL_ROOT = ROOT / "1.素材" / "批量剪辑"
PREPROCESS_ROOT = ROOT / "2.预处理" / "批量剪辑"
OUTPUT_ROOT = ROOT / "3.成品"
RUNTIME_ROOT = ROOT / ".lightflow" / "runtime"
CONFIG_PATH = RUNTIME_ROOT / "xry-dashboard.json"
HTML_PATH = Path(__file__).with_name("index.html")
LFW = "/srv/.lightflow/bin/lfw-xry"
VIDEO_EXTENSIONS = {".mp4", ".mov", ".m4v", ".mkv"}
PHOTO_EXTENSIONS = {".png", ".jpg", ".jpeg", ".webp"}


def run(args: list[str], timeout: float = 5) -> str:
    try:
        return subprocess.run(args, capture_output=True, text=True, timeout=timeout).stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        return ""


def read_json(path: Path, fallback: object) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return fallback


def load_config() -> dict[str, list[str]]:
    value = read_json(CONFIG_PATH, {})
    if not isinstance(value, dict):
        return {"enabled_tasks": [], "enabled_sources": []}
    return {key: sorted({item for item in value.get(key, []) if isinstance(item, str)})
            for key in ("enabled_tasks", "enabled_sources")}


def unit(name: str) -> dict[str, str]:
    raw = run(["systemctl", "--user", "show", name, "-p", "ActiveState", "-p", "SubState",
               "-p", "NextElapseUSecRealtime", "-p", "OnCalendar", "-p", "Persistent"])
    return {key: value for line in raw.splitlines() for key, _, value in [line.partition("=")] if key}


def progress(task: str, subject: str) -> dict[str, object]:
    raw = run([LFW, "run", "lightflow.xry_batch_control", "-i", "action=progress", "-i",
               f"task={task}", "-i", f"subject={subject}"], 8)
    try:
        start = raw.find("{")
        return json.loads(raw[start:]) if start >= 0 else {"summary": raw[-240:] or "暂不可用"}
    except (ValueError, TypeError):
        return {"summary": raw[-240:] or "暂不可用"}


def media_counts(path: Path) -> tuple[int, int]:
    files = [item for item in path.iterdir() if item.is_file()]
    return (sum(item.suffix.lower() in VIDEO_EXTENSIONS for item in files),
            sum(item.suffix.lower() in PHOTO_EXTENSIONS for item in files))


def sources() -> list[dict[str, object]]:
    if not MATERIAL_ROOT.is_dir():
        return []
    result = []
    for directory in sorted(MATERIAL_ROOT.rglob("*")):
        if not directory.is_dir() or not any(item.is_file() for item in directory.iterdir()):
            continue
        relative = directory.relative_to(MATERIAL_ROOT).as_posix()
        videos, photos = media_counts(directory)
        if not videos and not photos:
            continue
        task = (f"批量剪辑/{relative}" if "/" in relative
                else f"批量剪辑/未分类/{relative}")
        result.append({"name": relative, "relative": relative, "task": task,
                       "videos": videos, "photos": photos})
    return result


def tasks() -> list[dict[str, object]]:
    result = []
    if not PREPROCESS_ROOT.is_dir():
        return result
    for group in sorted(PREPROCESS_ROOT.iterdir()):
        if not group.is_dir():
            continue
        for batch in sorted(group.iterdir()):
            production = batch / ".pipeline" / "production"
            if not production.is_dir():
                continue
            task = f"批量剪辑/{group.name}/{batch.name}"
            subjects = []
            for directory in sorted(production.glob("S[0-9][0-9]")):
                gate = read_json(directory / "quality-gate.json", {})
                receipt = read_json(directory / "delivery-receipt.json", {})
                subjects.append({"subject": directory.name,
                                 "status": gate.get("status", "未完成"),
                                 "reason": gate.get("reason", ""),
                                 "receipt": receipt.get("status", ""),
                                 "progress": progress(task, directory.name)})
            result.append({"task": task, "group": group.name, "batch": batch.name,
                           "subjects": subjects})
    return result


def outputs() -> list[dict[str, object]]:
    if not OUTPUT_ROOT.is_dir():
        return []
    return [{"account": directory.name, "videos": len(list(directory.rglob("*.mp4"))),
             "path": str(directory)} for directory in sorted(OUTPUT_ROOT.iterdir()) if directory.is_dir()]


def state() -> dict[str, object]:
    return {
        "updated_at": dt.datetime.now(dt.timezone(dt.timedelta(hours=8))).isoformat(timespec="seconds"),
        "host": os.uname().nodename,
        "timer": unit("xry-auto-produce.timer"),
        "scheduler": unit("xry-auto-produce.service"),
        "lock": (ROOT / ".lightflow" / "xry-auto-produce.lock").exists(),
        "config": load_config(), "sources": sources(), "tasks": tasks(), "outputs": outputs(),
        "paths": {"runtime": str(RUNTIME_ROOT), "sources": str(MATERIAL_ROOT),
                  "preprocess": str(PREPROCESS_ROOT), "outputs": str(OUTPUT_ROOT),
                  "video_work_api": "http://127.0.0.1:7860", "dashboard": "http://0.0.0.0:8318"},
        "dependencies": {"orchestrator": "/srv/.lightflow/bin/lfw-xry",
                         "video_work_api": "video-work-api.service :7860",
                         "media": "ffmpeg + ffprobe", "asr": "FunClip / Paraformer",
                         "translation": "MADLAD-400-3B-MT", "scheduler": "systemd user timer"},
        "rules": ["去除开始/开拍等拍摄指令", "特写只用同车单独拍摄原片且语义匹配",
                  "没有合规特写时保留主体画面", "非标准目录自动进入批量剪辑/未分类",
                  "未分类保留纯图片空白封面", "ZE 中文+英文，RE 俄文+英文"],
    }


def selection(value: object, prefix: str) -> list[str]:
    if not isinstance(value, list):
        raise ValueError("selection must be an array")
    result = []
    for item in value:
        if (not isinstance(item, str) or not item or ".." in Path(item).parts
                or (prefix and not item.startswith(prefix)) or item.startswith("/")):
            raise ValueError("selection contains an unsafe path")
        result.append(item)
    return sorted(set(result))


class Handler(BaseHTTPRequestHandler):
    def reply(self, value: object, status: int = 200) -> None:
        body = json.dumps(value, ensure_ascii=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        if path == "/api/state":
            self.reply(state())
        elif path == "/":
            body = HTML_PATH.read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.reply({"error": "not found"}, 404)

    def do_POST(self) -> None:
        try:
            payload = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))) or b"{}")
            path = urlparse(self.path).path
            if path == "/api/config":
                value = {"enabled_tasks": selection(payload.get("enabled_tasks", []), "批量剪辑/"),
                         "enabled_sources": selection(payload.get("enabled_sources", []), "")}
                RUNTIME_ROOT.mkdir(parents=True, exist_ok=True)
                with tempfile.NamedTemporaryFile("w", dir=RUNTIME_ROOT, delete=False, encoding="utf-8") as handle:
                    json.dump(value, handle, ensure_ascii=False, indent=2)
                    temporary = handle.name
                os.replace(temporary, CONFIG_PATH)
                self.reply({"config": value})
            elif path == "/api/run":
                subprocess.Popen(["systemctl", "--user", "start", "xry-auto-produce.service"],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                self.reply({"started": True})
            else:
                self.reply({"error": "not found"}, 404)
        except (ValueError, OSError) as error:
            self.reply({"error": str(error)}, 400)

    def log_message(self, *_: object) -> None:
        return


if __name__ == "__main__":
    ThreadingHTTPServer((os.environ.get("XRY_DASHBOARD_BIND", "0.0.0.0"), 8318), Handler).serve_forever()
