# LightFlow Auto Editing

Source-controlled LightFlow workflows for automated video editing.

The first workflow, `lightflow.video_auto_edit_plan`, produces a structured edit
decision plan from clip metadata, narration/script notes, style guidance, and
delivery constraints. `lightflow.video_render_edit` declares the render
contract for applying that plan to clip files. The runnable renderer currently
lives in `scripts/render_edit.py` and uses ffmpeg.

## Planning Workflow

```bash
lfw run lightflow.video_auto_edit_plan \
  --input clips='[{"id":"a-roll-1","path":"a.mp4","start":0,"end":12}]' \
  --input brief='"60 second product recap"' \
  --input style='"fast educational social cut"' \
  --input constraints='{"aspect_ratio":"9:16","max_duration_seconds":60}'
```

HTTP:

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video_auto_edit_plan/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"clips":[{"id":"a-roll-1","path":"a.mp4","start":0,"end":12}],"brief":"60 second product recap","style":"fast educational social cut","constraints":{"aspect_ratio":"9:16","max_duration_seconds":60}}}'
```

## Render Workflow

```bash
lfw run lightflow.video_render_edit \
  --input edit_plan='@examples/demo/edit_plan.json' \
  --input output_path='"examples/output/auto-edit-demo.mp4"'
```

The LightFlow workflow records the render contract. To render the demo video
locally, run:

```bash
python3 scripts/render_edit.py \
  --plan examples/demo/edit_plan.json \
  --output examples/output/auto-edit-demo.mp4
```

The committed sample output is:

```text
examples/output/auto-edit-demo.mp4
```

## Layout

```text
workflows/auto_edit_plan/
  Cargo.toml
  src/lib.rs
  .agent/skills/lightflow-auto-edit-plan/SKILL.md
workflows/render_edit/
  Cargo.toml
  src/lib.rs
  .agent/skills/lightflow-render-edit/SKILL.md
scripts/render_edit.py
examples/demo/edit_plan.json
examples/output/auto-edit-demo.mp4
```
