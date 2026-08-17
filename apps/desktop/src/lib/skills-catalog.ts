export interface SkillDefinition {
  id: string;
  name: string;
  description: string;
  systemPrompt: string;
  category: "analysis" | "development" | "design" | "security" | "testing" | "devops";
}

export const SKILLS_CATALOG: readonly SkillDefinition[] = [
  {
    id: "hawk-graph",
    name: "HAWK Graph Memory",
    description: "Recall indexed project structure and changed files",
    category: "analysis",
    systemPrompt:
      "Use project_graph_structure and project_graph_query for focused context instead of re-scanning the whole project.",
  },
  {
    id: "project-analysis",
    name: "Project Analysis",
    description: "Explore structure and dependencies",
    category: "analysis",
    systemPrompt:
      "Analyze the project architecture: entry points, module boundaries, dependency graph, and key abstractions before making changes.",
  },
  {
    id: "git-review",
    name: "Git Review",
    description: "Understand changes and merge risks",
    category: "analysis",
    systemPrompt:
      "Review git history, branch differences, and identify merge conflicts or risky changes before committing.",
  },
  {
    id: "test-planning",
    name: "Test Planning",
    description: "Write executable acceptance tests",
    category: "testing",
    systemPrompt:
      "Plan and write comprehensive tests: unit, integration, and edge cases. Follow AAA pattern (Arrange, Act, Assert).",
  },
  {
    id: "security-review",
    name: "Security Review",
    description: "Spot secret and permission risks",
    category: "security",
    systemPrompt:
      "Review code for security vulnerabilities: hardcoded secrets, SQL injection, XSS, permission escalation, and unsafe operations.",
  },
  {
    id: "ui-ux-pro",
    name: "UI/UX Design Pro",
    description: "Apply modern UI/UX design principles",
    category: "design",
    systemPrompt:
      "Apply modern UI/UX principles: consistent spacing (4/8/16px grid), typography hierarchy, color theory with proper contrast, accessibility (WCAG 2.1 AA), responsive layouts, and meaningful micro-interactions.",
  },
  {
    id: "responsive-design",
    name: "Responsive Design",
    description: "Mobile-first responsive layouts",
    category: "design",
    systemPrompt:
      "Ensure all layouts work across mobile (375px), tablet (768px), and desktop (1024px+). Use CSS Grid, flexbox, container queries, and fluid typography with clamp().",
  },
  {
    id: "dark-mode",
    name: "Dark Mode Expert",
    description: "Theme switching without layout shifts",
    category: "design",
    systemPrompt:
      "Implement proper dark/light mode with CSS custom properties, color contrast ratios, smooth transitions, and no layout shifts when switching themes.",
  },
  {
    id: "animation-pro",
    name: "Animation & Motion",
    description: "Micro-interactions and transitions",
    category: "design",
    systemPrompt:
      "Add meaningful micro-interactions, transitions, and animations. Use CSS transitions, keyframes, and requestAnimationFrame for smooth 60fps motion. Respect prefers-reduced-motion.",
  },
  {
    id: "accessibility",
    name: "Accessibility (A11y)",
    description: "WCAG 2.1 AA compliance",
    category: "development",
    systemPrompt:
      "Ensure WCAG 2.1 AA compliance: proper ARIA labels, keyboard navigation, focus management, screen reader support, color contrast (4.5:1 minimum), and semantic HTML.",
  },
  {
    id: "performance",
    name: "Performance Optimization",
    description: "Core Web Vitals and runtime perf",
    category: "development",
    systemPrompt:
      "Optimize for Core Web Vitals: lazy loading, code splitting, image optimization, bundle analysis, render performance, and memory leak prevention.",
  },
  {
    id: "i18n-pro",
    name: "Internationalization Pro",
    description: "RTL/LTR and locale-aware UI",
    category: "development",
    systemPrompt:
      "Implement RTL/LTR support, plural rules, date/number formatting, locale detection, and translation management. Use logical CSS properties.",
  },
  {
    id: "api-design",
    name: "API Design",
    description: "RESTful APIs with proper patterns",
    category: "development",
    systemPrompt:
      "Design RESTful APIs with proper HTTP methods, status codes, pagination, error handling, rate limiting, and OpenAPI documentation.",
  },
  {
    id: "database-design",
    name: "Database Design",
    description: "Schemas, indexes, and migrations",
    category: "development",
    systemPrompt:
      "Design normalized schemas, write efficient queries, add proper indexes, handle migrations safely, and prevent N+1 queries.",
  },
  {
    id: "error-handling",
    name: "Error Handling Pro",
    description: "Comprehensive error patterns",
    category: "development",
    systemPrompt:
      "Implement comprehensive error handling: try/catch patterns, error boundaries, user-friendly messages, structured logging, and recovery strategies.",
  },
  {
    id: "code-review",
    name: "Code Review Expert",
    description: "SOLID, DRY, and maintainability",
    category: "development",
    systemPrompt:
      "Review code for: naming conventions, SOLID principles, DRY, test coverage, security vulnerabilities, and maintainability. Suggest specific improvements.",
  },
  {
    id: "refactoring",
    name: "Refactoring Master",
    description: "Clean code transformations",
    category: "development",
    systemPrompt:
      "Apply refactoring patterns: extract method, introduce parameter object, replace conditional with polymorphism, and other clean code techniques.",
  },
  {
    id: "documentation",
    name: "Documentation Writer",
    description: "JSDoc, READMEs, and inline docs",
    category: "development",
    systemPrompt:
      "Write clear JSDoc/TSDoc comments, README files, API documentation, and inline code comments that explain 'why' not 'what'.",
  },
  {
    id: "state-management",
    name: "State Management",
    description: "Clean state patterns",
    category: "development",
    systemPrompt:
      "Implement clean state patterns: reducers, selectors, immutability, derived state, and minimal re-renders. Prefer local state over global state.",
  },
  {
    id: "ci-cd",
    name: "CI/CD Pipeline",
    description: "Build, test, deploy automation",
    category: "devops",
    systemPrompt:
      "Set up GitHub Actions, GitLab CI, or similar: build, test, lint, deploy stages with proper caching, artifact management, and rollback strategies.",
  },
  {
    id: "docker-pro",
    name: "Docker & Containers",
    description: "Optimized containerization",
    category: "devops",
    systemPrompt:
      "Write optimized Dockerfiles with multi-stage builds, minimal image size, proper .dockerignore, health checks, and docker-compose configs.",
  },
  {
    id: "monitoring",
    name: "Monitoring & Logging",
    description: "Structured logging and metrics",
    category: "devops",
    systemPrompt:
      "Add structured logging (JSON format), error tracking (Sentry-style), performance metrics, health endpoints, and alerting hooks.",
  },
  {
    id: "git-workflow",
    name: "Git Workflow",
    description: "Branching, commits, and changelogs",
    category: "devops",
    systemPrompt:
      "Manage branches, rebase, squash, resolve merge conflicts, write conventional commits (feat/fix/chore), and maintain changelogs.",
  },
  {
    id: "testing-pro",
    name: "Testing Pro",
    description: "Unit, integration, and E2E tests",
    category: "testing",
    systemPrompt:
      "Write unit, integration, and E2E tests. Use proper mocking, test isolation, coverage analysis, AAA pattern, and test-driven development when appropriate.",
  },
] as const;

export const SKILL_IDS: readonly string[] = SKILLS_CATALOG.map((s) => s.id);

export function getSkillById(id: string): SkillDefinition | undefined {
  return SKILLS_CATALOG.find((s) => s.id === id);
}
