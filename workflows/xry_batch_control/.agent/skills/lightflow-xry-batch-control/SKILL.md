---
name: lightflow-xry-batch-control
description: Inspect, reject, rerender, and package XRY batch subjects through the restricted Rust-native LightFlow control workflow.
version: 0.6.0
---

# XRY batch control through LightFlow

Use only `lightflow.xry_batch_control`: `status`, `reject`, `audit`, `cover`,
`produce`, or `package`. `task` must be the frozen relative path
`批量剪辑/<account-group>/<batch>`; account-group names may contain spaces, for
example `批量剪辑/皮卡严选 走全球/7.23批量`. Pass it as one text input—do not split,
shell-escape, or invoke the XRY worker directly.

`reject`, `audit`, `cover`, `produce`, and `package` require `subject`. Bad
media must be rejected before rerendering. `package` fails closed unless the
current quality gate passes and is the only delivery boundary. Cover specs are
validated against the registered reference for that account group; missing,
unregistered, outside-group, default, or black covers are rejected.
Each `cover-spec.json` must also explicitly contain a CJK `headline_zh` for
ZE and a Cyrillic `headline_ru` for RE. Text is never inferred from a video ID,
caption, generic headline, or EDL title; both required fields validate before
either cover is rendered.

```bash
lfw run lightflow.xry_batch_control \
  --input action='"status"' \
  --input task='"批量剪辑/皮卡严选 走全球/7.23批量"'
```

## API Usage

```bash
curl -sS -X POST http://127.0.0.1:5174/workflows/lightflow.xry_batch_control/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"action":"status","task":"批量剪辑/皮卡严选 走全球/7.23批量"}}'
```
