# HAWK Agent Engine — Research Notes and v7 Architecture

## Why the current agent became unstable

The failures observed during HAWK's raw Qwen3-Coder-Next-Base experiments are mostly scaffold failures rather than model-capability failures:

- Native tool-call syntax from a base model is not stable enough to be the execution protocol.
- The model was asked to decide both *what to do* and *how the runtime should manage state, permissions, verification, retries, and completion*.
- Repeated reads and repeated failed commands were counted as progress.
- Verification (`test -> lint -> build`) was model-driven instead of runtime-driven.
- Completion depended too heavily on the model choosing a correct `finish` action.
- There was no durable typed execution record comparable to an event stream/checkpoint log.

## Research synthesis

### ReAct

ReAct's important idea is an interleaved **action -> observation -> updated decision** loop. The environment's observation must be fed back into the next decision rather than allowing a model to hallucinate that an action happened.

Source: Yao et al., *ReAct: Synergizing Reasoning and Acting in Language Models* (2022).

### SWE-agent and the Agent-Computer Interface (ACI)

SWE-agent demonstrates that the interface exposed to the model materially affects coding-agent performance. Useful ACI properties include concise repository search, controlled file viewing/editing, lint feedback, and explicit command observations.

Source: Yang et al., *SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering* (NeurIPS 2024), plus the official SWE-agent ACI documentation.

### mini-SWE-agent

The newer mini-SWE-agent intentionally simplifies the scaffold:

- a linear action/observation history;
- independent subprocess executions rather than a fragile persistent shell;
- a small, inspectable control loop;
- no dependency on provider-native function calling for the core workflow.

The key engineering lesson for HAWK is that **a smaller deterministic runtime around a strong model is usually more stable than a large prompt-driven orchestration layer**.

Source: official SWE-agent/mini-swe-agent repository.

### OpenHands

OpenHands separates the agent from the runtime and models execution as typed Actions and Observations over an event stream. The runtime owns filesystem/shell/browser execution and isolation; the model/controller does not directly mutate the host.

Source: official OpenHands Runtime Architecture and Event System documentation.

### Durable execution / checkpointing

Long-running agents should checkpoint state at step boundaries. A failed node should not force all previous successful work to run again. State should be JSON-serializable, replayable, and observable.

Reference implementation pattern: LangGraph persistence/checkpointing documentation.

### Constrained structured generation

llama-cpp-python supports `response_format` with JSON/JSON-Schema constrained decoding. This is much more reliable for a raw/base model than hoping it consistently emits one of several textual tool-call dialects.

Source: official llama-cpp-python and llama.cpp documentation.

## HAWK v7 design principles

1. **One canonical action protocol**
   - The model emits a JSON object constrained by a JSON schema.
   - HAWK no longer treats arbitrary Qwen `<tool_call>` markup as the primary protocol.
   - Legacy parsing remains only as a fallback.

2. **Explicit state machine**
   - `Inspect -> Act -> Verify -> Repair -> Verify -> Complete`.
   - The model cannot decide to skip required verification.
   - HAWK can finish deterministically once all requirements pass.

3. **Runtime-owned verification**
   - For Node projects HAWK runs requested checks itself in order: `npm test`, `npm run lint`, `npm run build`.
   - On failure, the exact observation goes to the model in Repair phase.
   - After an edit, HAWK retries the failed check automatically.

4. **Typed Action/Observation boundary**
   - Model: selects an action.
   - Runtime: validates policy, executes, returns an observation.
   - Controller: updates state and decides the next phase.

5. **No fake progress**
   - Duplicate unchanged reads are served from cache or rejected.
   - Consecutive inspection budget is limited.
   - Re-running an identical failed command without a project change is blocked.

6. **Workspace confinement**
   - All file operations resolve inside the active workspace.
   - Absolute paths are normalized only when they are inside that workspace.

7. **Independent bounded commands**
   - Each command is a fresh child process with a timeout and kill-on-drop behavior.
   - Bare interactive `node`/`python` and long-running `dev/start/serve` commands are rejected in autonomous verification.

8. **Purpose-built ACI**
   - `list_files`, `search_text`, `read_file`, `write_file`, `replace_in_file`, `run_command`, `git_status`, and browser control.
   - Search output is concise and file operations return explicit observations.

9. **Event log / checkpoint foundation**
   - Every action, observation, verification result, and completion is appended as structured JSONL.
   - This gives reproducible debugging and is the foundation for future crash-resume support.

10. **The raw model is not the workflow engine**
    - Qwen should solve coding decisions.
    - HAWK owns correctness properties: parsing, permissions, state, retries, loop detection, verification order, and completion.

## Target control flow

```text
User task
  -> Controller builds state
  -> Deterministic preflight / required verification detection
  -> Model selects ONE schema-constrained action
  -> Policy validates action
  -> Runtime executes action
  -> Typed observation is recorded
  -> Controller updates state
     -> if check failed: Repair
     -> if edit completed: retry failed check
     -> if verification pending: run next check
     -> if all requirements pass: Complete
     -> otherwise: next model action
```

The design intentionally keeps browser/computer-control actions as a separate runtime boundary. UI automation should produce an observation (snapshot/screenshot/result) that the controller records before the model chooses the next action.
