# LightFlow Auto Editing

Source-controlled LightFlow workflows for automated video editing.

The first workflow, `lightflow.video.auto_edit_plan`, produces a structured edit
decision plan from clip metadata, narration/script notes, style guidance, and
delivery constraints. It is intended as the planning stage before a renderer or
ffmpeg-backed workflow applies cuts, transitions, captions, and audio ducking.

## Workflow

```bash
lfw run lightflow.video.auto_edit_plan \
  --input clips='[{"id":"a-roll-1","path":"a.mp4","start":0,"end":12}]' \
  --input brief='"60 second product recap"' \
  --input style='"fast educational social cut"' \
  --input constraints='{"aspect_ratio":"9:16","max_duration_seconds":60}'
```

HTTP:

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video.auto_edit_plan/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"clips":[{"id":"a-roll-1","path":"a.mp4","start":0,"end":12}],"brief":"60 second product recap","style":"fast educational social cut","constraints":{"aspect_ratio":"9:16","max_duration_seconds":60}}}'
```

## Layout

```text
workflows/video/auto_edit_plan/
  Cargo.toml
  src/lib.rs
  .agent/skills/lightflow-auto-edit-plan/SKILL.md
```

