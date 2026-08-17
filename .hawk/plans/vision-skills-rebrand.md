# HAWK Code — Vision Model + Skills Overhaul + Hawk K3 Rebrand

## Overview
Three interconnected features:
1. **Vision Pipeline**: Deploy Qwen3-VL-8B-Instruct on a second Modal GPU → two-model collaboration (vision → code)
2. **Agent-Driven Skills**: Remove Skills page, keep all 5 + add ~15 new skills, agent auto-decides when to use them with user approval
3. **Hawk K3 Rebrand**: Remove all "Qwen" references, rename to "Hawk K3"

---

## Phase 1: Hawk K3 Rebrand (Simplest — do first)

### Files to change

#### `packages/shared-types/src/index.ts`
- Rename `QwenModelId` → `HawkModelId`
- Rename `QwenModel` → `HawkModel`
- Rename `QWEN_MODELS` → `HAWK_MODELS`
- Change `displayName` from `"Qwen3 Coder 30B · Modal GPU"` → `"Hawk K3 · Coder"`
- Change model `id` from `"qwen3-coder-30b-a3b-instruct"` → `"hawk-k3-coder"`
- Update `DEFAULT_MODEL_ID` accordingly
- Keep the old `"qwen3-coder-30b-a3b-instruct"` as a legacy alias in `QwenModelId` type for backend compatibility

#### `apps/desktop/src/lib/qwen-model.ts`
- Rename file → `hawk-model.ts`
- Rename `resolveChatModel` → `resolveHawkModel`
- Update all imports referencing this file

#### `apps/desktop/src/store/workbench.ts`
- Rename `selectedQwenModel` → `selectedHawkModel`
- Rename `qwenBaseUrl` → `hawkBaseUrl`
- Update all references in the store and components

#### `apps/desktop/src/components/Composer.tsx`
- Update all `qwen` references to `hawk`
- Change `resolveChatModel` → `resolveHawkModel`

#### `apps/desktop/src/lib/ipc.ts`
- Rename `streamQwenChat` → `streamHawkChat`
- Rename `streamQwenAgent` → `streamHawkAgent`
- Keep Tauri command names as `qwen_chat` / `qwen_agent` (backend compatibility) OR update the Rust backend too

#### `apps/desktop/src/i18n.ts`
- Change `"models.quality": "Best quality"` → keep as is
- Remove/update `"composer.visionFallback"` text to remove "Qwen 3.7 Max" references
- Add `"composer.hawkK3": "Hawk K3"` if needed

#### `infra/modal/app.py`
- Keep `MODEL_ID = "qwen3-coder-30b-a3b-instruct"` internally (actual model file)
- Change the API response `model` field to `"hawk-k3-coder"` for display

#### All component files that import from shared-types
- Update type references: `QwenModelId` → `HawkModelId`, etc.
- Files: Composer.tsx, SettingsView.tsx, any model selector components

### NOT changing
- The actual llama.cpp model file or GGUF weights (still Qwen3-Coder-30B under the hood)
- The Tauri Rust backend command names (keep `qwen_chat` / `qwen_agent` to avoid Rust recompilation unless necessary)
- The Modal deployment URL (stays the same)

---

## Phase 2: Vision Pipeline (Modal GPU #2)

### Architecture
```
User sends image + text
  → Frontend detects image attachment
  → Sends to Vision endpoint (Modal GPU #2)
  → Vision model (Qwen3-VL-8B) returns detailed text description
  → Frontend appends description to user message
  → Sends combined text to Code endpoint (Modal GPU #1, Hawk K3)
  → Code model implements changes
```

### 2a. New Modal Deployment: `infra/modal/vision.py`

Deploy a second Modal app with Qwen3-VL-8B-Instruct:

```python
# Model config
MODEL_REPO = "Qwen/Qwen3-VL-8B-Instruct-GGUF"
MODEL_FILE = "Qwen3VL-8B-Instruct-Q8_0.gguf"  # ~8.7GB
MMPROJ_FILE = "mmproj-Qwen3VL-8B-Instruct-F16.gguf"  # ~0.9GB
GPU = "L4"  # 24GB, plenty of room
```

Endpoints:
- `POST /v1/chat/completions` — accepts messages with `image_url` content blocks
- Uses `llama.cpp` Python bindings with `--mmproj` flag for vision support
- Returns text description of the image

Key differences from code model:
- Needs `mmproj` (vision encoder) file loaded alongside the LLM
- llama.cpp loads vision models with: `llama_model_params` + separate `mmproj` path
- The `create_chat_completion` in `llama_cpp` Python library supports multimodal messages

### 2b. Frontend Vision Orchestration

#### New file: `apps/desktop/src/lib/vision.ts`
```typescript
export interface VisionConfig {
  baseUrl: string;
  model: string;
}

export async function analyzeImage(
  config: VisionConfig,
  imageDataUrl: string,
  userPrompt: string,
): Promise<string> {
  // Send to vision endpoint
  // Returns detailed text description + modification request
}
```

#### Changes to `Composer.tsx` `runConversation()`
1. Before sending to the agent, check if the user message has image attachments
2. If yes:
   a. Send each image to the vision model with the user's text prompt
   b. Get back a detailed text description
   c. Prepend the description to the user message: `"Image analysis:\n{description}\n\nUser request: {prompt}"`
   d. Remove the image attachments from the message (code model can't see images)
   e. Send the text-only enriched message to the code model
3. If no images: proceed as normal

#### Store changes (`workbench.ts`)
- Add `visionBaseUrl` and `visionModelId` fields
- Default: `"https://mjakcon8-hawk-code--hawk-vision.modal.run/v1"`
- Model: `"hawk-k3-vision"`

### 2c. UI Feedback
- When processing an image, show a notice: "Analyzing image with Hawk K3 Vision..."
- After vision analysis completes, show: "Image analyzed. Processing with Hawk K3 Coder..."
- Both notices use the existing `setNotice()` mechanism

### 2d. Settings
- Add Vision model URL field in SettingsView (optional, defaults to Modal deployment)
- Add a toggle: "Enable vision pipeline" (on by default)

---

## Phase 3: Agent-Driven Skills

### 3a. Remove Skills Page

#### `apps/desktop/src/components/Sidebar.tsx`
- Remove `{ view: "skills", key: "skills", icon: Sparkles }` from `primaryNavigation` array (line 34)

#### `apps/desktop/src/components/WorkbenchView.tsx`
- Remove the lazy import of `SkillsView`
- Remove `case "skills"` from the switch statement

#### `apps/desktop/src/store/workbench.ts`
- Remove `"skills"` from `RailView` type
- Keep `enabledSkills` array and `toggleSkill` in the store (used by agent internally)
- Add `pendingSkillApproval: { skill: string; message: string } | null` for skill approval UI

#### `apps/desktop/src/i18n.ts`
- Keep skill name/hint translations (used in agent notifications)
- Remove `"nav.skills"` key
- Remove `"slash.skills"` and `"slash.skill"` slash command keys

#### `apps/desktop/src/components/Composer.tsx`
- Remove `/skills` and `/skill` slash command handlers

### 3b. Expand Skills Catalog

#### New file: `apps/desktop/src/lib/skills-catalog.ts`
A comprehensive catalog of ~20+ skills the agent can use:

```typescript
export interface SkillDefinition {
  id: string;
  name: string;          // Display name
  description: string;   // What this skill does
  systemPrompt: string;  // Extra instructions injected when skill is active
  category: "analysis" | "development" | "design" | "security" | "testing" | "devops";
}

export const SKILLS_CATALOG: SkillDefinition[] = [
  // Existing 5
  { id: "hawk-graph", name: "HAWK Graph Memory", ... },
  { id: "project-analysis", name: "Project Analysis", ... },
  { id: "git-review", name: "Git Review", ... },
  { id: "test-planning", name: "Test Planning", ... },
  { id: "security-review", name: "Security Review", ... },
  
  // New skills
  { id: "ui-ux-pro", name: "UI/UX Design Pro", category: "design",
    systemPrompt: "Apply modern UI/UX principles: spacing, typography, color theory, accessibility (WCAG), responsive design, motion design, and user flow optimization." },
  { id: "responsive-design", name: "Responsive Design", category: "design",
    systemPrompt: "Ensure all layouts work across mobile (375px), tablet (768px), desktop (1024px+). Use CSS Grid, flexbox, container queries, and fluid typography." },
  { id: "dark-mode", name: "Dark Mode Expert", category: "design",
    systemPrompt: "Implement proper dark mode with CSS custom properties, color contrast ratios, and theme switching without layout shifts." },
  { id: "animation-pro", name: "Animation & Motion", category: "design",
    systemPrompt: "Add meaningful micro-interactions, transitions, and animations. Use CSS transitions, keyframes, and requestAnimationFrame for smooth 60fps motion." },
  { id: "accessibility", name: "Accessibility (A11y)", category: "development",
    systemPrompt: "Ensure WCAG 2.1 AA compliance: proper ARIA labels, keyboard navigation, focus management, screen reader support, and color contrast." },
  { id: "performance", name: "Performance Optimization", category: "development",
    systemPrompt: "Optimize for Core Web Vitals: lazy loading, code splitting, image optimization, bundle analysis, and render performance." },
  { id: "i18n-pro", name: "Internationalization Pro", category: "development",
    systemPrompt: "Implement RTL/LTR support, plural rules, date/number formatting, locale detection, and translation management." },
  { id: "api-design", name: "API Design", category: "development",
    systemPrompt: "Design RESTful APIs with proper HTTP methods, status codes, pagination, error handling, and OpenAPI documentation." },
  { id: "database-design", name: "Database Design", category: "development",
    systemPrompt: "Design normalized schemas, write efficient queries, add proper indexes, and handle migrations safely." },
  { id: "error-handling", name: "Error Handling Pro", category: "development",
    systemPrompt: "Implement comprehensive error handling: try/catch patterns, error boundaries, user-friendly messages, logging, and recovery strategies." },
  { id: "code-review", name: "Code Review Expert", category: "development",
    systemPrompt: "Review code for: naming conventions, SOLID principles, DRY, test coverage, security vulnerabilities, and maintainability." },
  { id: "refactoring", name: "Refactoring Master", category: "development",
    systemPrompt: "Apply refactoring patterns: extract method, introduce parameter object, replace conditional with polymorphism, and other clean code techniques." },
  { id: "documentation", name: "Documentation Writer", category: "development",
    systemPrompt: "Write clear JSDoc/TSDoc comments, README files, API documentation, and inline code comments that explain 'why' not 'what'." },
  { id: "ci-cd", name: "CI/CD Pipeline", category: "devops",
    systemPrompt: "Set up GitHub Actions, GitLab CI, or similar: build, test, lint, deploy stages with proper caching and artifact management." },
  { id: "docker-pro", name: "Docker & Containers", category: "devops",
    systemPrompt: "Write optimized Dockerfiles, docker-compose configs, multi-stage builds, and health checks." },
  { id: "monitoring", name: "Monitoring & Logging", category: "devops",
    systemPrompt: "Add structured logging, error tracking, performance metrics, and alerting hooks." },
  { id: "state-management", name: "State Management", category: "development",
    systemPrompt: "Implement clean state patterns: reducers, selectors, immutability, derived state, and minimal re-renders." },
  { id: "testing-pro", name: "Testing Pro", category: "testing",
    systemPrompt: "Write unit, integration, and E2E tests. Use proper mocking, test isolation, coverage analysis, and AAA pattern." },
  { id: "e2e-testing", name: "E2E Testing", category: "testing",
    systemPrompt: "Write Playwright/Cypress tests: page objects, selectors, assertions, fixtures, and visual regression testing." },
  { id: "git-workflow", name: "Git Workflow", category: "devops",
    systemPrompt: "Manage branches, rebase, squash, resolve merge conflicts, write conventional commits, and manage changelogs." },
];
```

### 3c. Agent-Driven Skill Selection

#### Changes to `Composer.tsx` system prompt (lines 221-226)
Replace the simple skill injection with an intelligent skill directive:

```
You have access to specialized skills that enhance your capabilities.
Available skills: ${allSkillsFromCatalog.join(", ")}.

When you determine that a specific skill would significantly improve your response:
1. FIRST, announce which skill you want to use and WHY: [SKILL_REQUEST: skill-id] reason
2. WAIT for user approval before applying the skill
3. Once approved, apply the skill's specialized knowledge

Never use a skill without announcing it first. The user may decline.
```

#### Changes to `agent-stages.ts`
- Add `"skill-request"` to `AgentStage` type
- Map the `[SKILL_REQUEST: ...]` pattern in the model's output to this stage

#### Changes to `TasksView.tsx`
- Add a `SkillRequestCard` component that appears when the agent wants to use a skill
- Shows: skill name, description, reason the agent wants it
- Two buttons: "Approve" (adds skill to `enabledSkills` and continues) / "Decline" (removes skill request and continues without it)

#### Changes to `store/workbench.ts`
- Add `pendingSkillRequest: { skillId: string; reason: string } | null`
- Add `approveSkill(skillId: string)` — adds to `enabledSkills`, clears pending, resumes agent
- Add `declineSkill()` — clears pending, resumes agent without the skill

### 3d. Skill Request Detection in Agent Output

#### New utility: `apps/desktop/src/lib/skill-request.ts`
```typescript
export interface SkillRequest {
  skillId: string;
  reason: string;
}

export function parseSkillRequest(text: string): SkillRequest | null {
  const match = text.match(/\[SKILL_REQUEST:\s*(\S+)\]\s*(.*)/);
  if (!match) return null;
  return { skillId: match[1], reason: match[2].trim() };
}
```

#### Integration in streaming
- In `TasksView.tsx` or `App.tsx`, as agent deltas arrive, check for `[SKILL_REQUEST: ...]` pattern
- When detected, pause the agent, show the approval card, wait for user response
- On approval: inject the skill's `systemPrompt` into the next message and resume
- On decline: resume without the skill

---

## Implementation Order

1. **Phase 1 (Rebrand)** — 1-2 hours
   - Simple find-and-replace across TypeScript files
   - Update shared types, store, components
   - Run tests to verify nothing breaks

2. **Phase 3 (Skills)** — 2-3 hours
   - Create skills catalog
   - Remove Skills page
   - Add skill request UI
   - Update system prompt
   - Run tests

3. **Phase 2 (Vision)** — 3-4 hours
   - Deploy Modal vision endpoint
   - Create vision.ts
   - Modify Composer to orchestrate two models
   - Add UI feedback
   - Test with real images

---

## Risks & Considerations

1. **Modal vision deployment**: llama.cpp Python bindings need the `mmproj` file. Need to verify the exact Python API for loading vision models with `llama-cpp-python`.
2. **Vision latency**: Qwen3-VL-8B on L4 should be ~2-5 seconds for image analysis. Total pipeline: ~5-10 seconds before code model starts.
3. **Skill approval UX**: The agent needs to be instructed clearly in the system prompt about the `[SKILL_REQUEST: ...]` format. Without this, it won't use the mechanism.
4. **Backward compatibility**: The old `"qwen3-coder-30b-a3b-instruct"` model ID must still work for the backend. Only the display name changes.
