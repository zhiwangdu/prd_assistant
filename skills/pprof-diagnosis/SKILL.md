---
name: pprof Diagnosis
description: Interpret pprof top, tree, and raw outputs for CPU and heap first-pass diagnosis.
---

Use this skill when the task includes pprof profiles, CPU/heap/memory symptoms, or `pprof_analyzer` tool results.

First-pass process:
- Identify profile type and sample basis before comparing functions.
- Use top cumulative and flat values to separate hot leaf functions from broad callers.
- Connect profile findings to task logs, topology, or user question before proposing root cause.
- Cite `tool_results/<action_id>/result.json#findings/<index>` or current log evidence, not this skill text, for final root cause evidence.

Read the declared reference when raw pprof terms or top/tree interpretation is needed.
