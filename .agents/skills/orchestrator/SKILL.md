---
name: orchestrator
description: Run a task through implementer → reviewer → fixer, each a subagent with a pinned model and effort, repeating review and fix until the verdict is clean. The reviewer is one agent kept across rounds.
disable-model-invocation: true
argument-hint: <task text or task file> [max-rounds]
---

You are a **relay**: you run the script below exactly, pass paths between stages, and match verdict lines as strings. Every judgement belongs to a stage. You read no code and write none; the only file you write is the task copy in step 1.

## Stages

The single source of truth for who runs each stage. Every Agent call MUST include both model and effort parameters, copied from the stage row. Before sending any Agent call, check that both are set; a call without them is invalid.

| Stage       | Skill file                            | model  | effort |
| ----------- | ------------------------------------- | ------ | ------ |
| implementer | `.agents/skills/implementer/SKILL.md` | sonnet | high   |
| reviewer    | `.agents/skills/reviewer/SKILL.md`    | opus   | xhigh  |
| fixer       | `.agents/skills/fixer/SKILL.md`       | sonnet | medium |

Spawn every stage as `subagent_type: general-purpose` in the foreground. Brief it with exactly this, filled in, nothing added:

```
Read <skill file> and follow it exactly.
Inputs:
- <name>: <value>
```

The reviewer is spawned once, in round 1; keep the agent ID the Agent call returns. From round 2, do not spawn a new reviewer: `SendMessage` to that same agent with exactly this, filled in, nothing added:

```
Round <k>. The fixer has appended its outcomes under `## Fixes` in <previous review file>.
Reread every file the fixes touched, then hunt again for bugs and smells across the whole change since <base>, not only the reread files.
Report only real issues; leave out cosmetic points and technicalities. Finding nothing is a valid outcome.
Apply step 3 of the skill to the previous review, write <review file> in the same format, and return the verdict line.
```

## Script

1. **Set up the run.**
   - `base` = `git rev-parse --short HEAD`.
   - `run` = `.agents/scratch/runs/<base>/`.
   - Write `<run>task.md`: the task text verbatim, or `cp` of the task file.
   - `max` = the max-rounds argument, else 3.

2. **Implementer.** Inputs: `task: <run>task.md`.

3. For `k` = 1 to `max`:
   1. **Reviewer.** k = 1: spawn it with inputs `base`, `task: <run>task.md`, `round: 1`, `review file: <run>review-1.md`. k > 1: send the round message above to the round-1 reviewer, with `previous review file: <run>review-<k-1>.md` and `review file: <run>review-<k>.md`.
   2. Read the one line of `<run>review-<k>.md` that starts with `VERDICT: `.
   3. `VERDICT: CLEAN` or `VERDICT: STUCK` → step 4.
   4. k = `max` → step 4.
   5. **Fixer.** Inputs: `review file: <run>review-<k>.md`, `task: <run>task.md`.

4. **Report**, verbatim and in this order:
   - Each stage's returned text, labelled `implementer`, `reviewer k`, `fixer k`.
   - The last verdict line.
   - The run folder path.
   - `git diff --stat <base>` and `git status --porcelain`.

## Stop

When a stage errors, the reviewer agent can no longer be messaged, or its review file or verdict line is missing, stop: print the step number, the stage's returned text verbatim, and the run folder path.
