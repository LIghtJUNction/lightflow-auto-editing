#!/usr/bin/env python3
"""Render a small edit decision plan with ffmpeg.

This renderer is intentionally conservative: it reads source clip paths from a
JSON plan, trims each segment, normalizes all segments to one output frame size,
burns a short title overlay, concatenates the results, and writes an MP4.
"""

from __future__ import annotations

import argparse
import json
import shlex
import subprocess
import tempfile
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Render a LightFlow edit plan.")
    parser.add_argument("--plan", required=True, type=Path, help="Path to edit_plan JSON.")
    parser.add_argument("--output", required=True, type=Path, help="Destination MP4 path.")
    parser.add_argument("--workdir", type=Path, default=None, help="Base directory for relative clip paths.")
    args = parser.parse_args()

    plan_path = args.plan.resolve()
    base_dir = args.workdir.resolve() if args.workdir else plan_path.parent.parent.parent
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    timeline = plan.get("timeline") or []
    if not timeline:
        raise SystemExit("edit plan must include a non-empty timeline array")

    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    settings = plan.get("output") or {}
    width = int(settings.get("width", 720))
    height = int(settings.get("height", 1280))
    fps = int(settings.get("fps", 30))

    with tempfile.TemporaryDirectory(prefix="lightflow-render-") as temp_name:
        temp_dir = Path(temp_name)
        rendered_segments = []
        for index, segment in enumerate(timeline):
            rendered = temp_dir / f"segment-{index:03}.mp4"
            render_segment(segment, rendered, base_dir, width, height, fps)
            rendered_segments.append(rendered)

        concat_file = temp_dir / "concat.txt"
        concat_file.write_text(
            "".join(f"file {shlex.quote(str(path))}\n" for path in rendered_segments),
            encoding="utf-8",
        )
        run([
            "ffmpeg",
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            str(concat_file),
            "-c",
            "copy",
            str(output),
        ])

    print(json.dumps({
        "video_path": str(output),
        "segments": len(timeline),
        "duration_seconds": round(sum(segment_duration(segment) for segment in timeline), 3),
    }, indent=2))
    return 0


def render_segment(segment: dict, output: Path, base_dir: Path, width: int, height: int, fps: int) -> None:
    source = resolve_path(base_dir, segment["path"])
    if not source.exists():
        raise SystemExit(f"source clip does not exist: {source}")
    start = float(segment.get("start", 0))
    duration = segment_duration(segment)
    title = str(segment.get("title") or segment.get("clip_id") or "").replace(":", "\\:")
    subtitle = str(segment.get("subtitle") or "").replace(":", "\\:")
    draw_title = (
        "drawtext=fontcolor=white:fontsize=42:"
        "box=1:boxcolor=black@0.55:boxborderw=18:"
        f"text='{escape_drawtext(title)}':x=40:y=70"
    )
    draw_subtitle = (
        "drawtext=fontcolor=white:fontsize=28:"
        "box=1:boxcolor=black@0.35:boxborderw=12:"
        f"text='{escape_drawtext(subtitle)}':x=40:y=h-150"
    )
    filters = [
        f"scale={width}:{height}:force_original_aspect_ratio=decrease",
        f"pad={width}:{height}:(ow-iw)/2:(oh-ih)/2",
        "setsar=1",
        draw_title,
    ]
    if subtitle:
        filters.append(draw_subtitle)
    run([
        "ffmpeg",
        "-y",
        "-hide_banner",
        "-loglevel",
        "error",
        "-ss",
        f"{start:.3f}",
        "-i",
        str(source),
        "-t",
        f"{duration:.3f}",
        "-vf",
        ",".join(filters),
        "-r",
        str(fps),
        "-an",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "23",
        "-pix_fmt",
        "yuv420p",
        str(output),
    ])


def segment_duration(segment: dict) -> float:
    if "duration" in segment:
        return max(0.1, float(segment["duration"]))
    start = float(segment.get("start", 0))
    end = float(segment.get("end", start + 1))
    return max(0.1, end - start)


def resolve_path(base_dir: Path, value: str) -> Path:
    path = Path(value)
    return path if path.is_absolute() else base_dir / path


def escape_drawtext(value: str) -> str:
    return value.replace("\\", "\\\\").replace("'", "\\'").replace("%", "\\%")


def run(command: list[str]) -> None:
    subprocess.run(command, check=True)


if __name__ == "__main__":
    raise SystemExit(main())
