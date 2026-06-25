---
name: LightFlow Video Auto Edit Plan
description: Use this skill when creating, running, or modifying the lightflow.video.auto_edit_plan workflow for automated video editing plans.
version: 0.1.0
---

# LightFlow Video Auto Edit Plan

Use `lightflow.video.auto_edit_plan` to turn clip metadata and a human editing
brief into a structured edit decision plan. The workflow is a source-controlled
planning node; rendering can be added later as a separate ffmpeg/runtime-backed
workflow that consumes `edit_plan`.

## Contract

- Workflow id: `lightflow.video.auto_edit_plan`
- Required input `clips`: JSON array of source clip records.
- Required input `brief`: human editing goal, story outline, or script notes.
- Optional input `style`: editing style guidance. Defaults to `clean social edit`.
- Optional input `constraints`: JSON delivery constraints. Defaults to `{}`.
- Output `edit_plan`: JSON edit decision plan.
- Output `summary`: readable planning summary.

## CLI Usage

```bash
lfw run lightflow.video.auto_edit_plan \
  --input clips='[{"id":"intro","path":"intro.mp4","start":0,"end":8,"tags":["hook"]}]' \
  --input brief='"Cut a concise launch recap with a strong opening hook."' \
  --input style='"fast social product edit"' \
  --input constraints='{"aspect_ratio":"9:16","max_duration_seconds":45,"captions":true}'
```

## API Usage

Start `lfw serve`, then call the workflow through the shared HTTP run contract:

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.video.auto_edit_plan/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"clips":[{"id":"intro","path":"intro.mp4","start":0,"end":8,"tags":["hook"]}],"brief":"Cut a concise launch recap with a strong opening hook.","style":"fast social product edit","constraints":{"aspect_ratio":"9:16","max_duration_seconds":45,"captions":true}}}'
```

## Change Notes

When changing inputs, outputs, runtime behavior, or common commands, update this
skill in the same change so agents can safely run and inspect the workflow.
