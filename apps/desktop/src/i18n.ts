import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const en = {
  "top.home": "Home",
  "top.workspace": "No workspace open",
  "top.localWorkspace": "Local workspace",
  "top.safe": "Ask for approval",
  "top.stop": "Stop all",
  "top.toggleSidebar": "Toggle sidebar",
  "top.nothingRunning": "No process is currently running.",
  "top.stopped": "All tasks stopped.",
  "top.running": "Hawk K3 is running",
  "top.ready": "Ready",
  "top.tokens": "tokens",
  "sidebar.newTask": "New task",
  "sidebar.search": "Search or command",
  "sidebar.navigation": "Main navigation",
  "sidebar.projects": "Projects",
  "sidebar.addProject": "Add or open a project",
  "sidebar.noProject": "No project open",
  "sidebar.noSessions": "No conversations yet",
  "sidebar.noSessionsHint": "Open a project and start your first task.",
  "sidebar.workspaceChat": "Project conversation",
  "sidebar.workspaceChatHint": "The current chat is scoped to {{project}}.",
  "sidebar.generalChat": "General chat",
  "sidebar.generalChatHint": "Chat without a project workspace",
  "sidebar.projectChats": "Project chats",
  "sidebar.generalChats": "Chats",
  "sidebar.newChat": "New chat",
  "sidebar.renameChat": "Rename chat",
  "sidebar.deleteChat": "Delete chat",
  "sidebar.chatOptions": "Chat options",
  "sidebar.removeProject": "Remove project from HAWK",
  "sidebar.messageCount": "{{count}} messages",
  "sidebar.emptyChat": "No messages yet",
  "nav.tasks": "Tasks",
  "nav.workspaces": "Workspaces",
  "nav.git": "Git changes",
  "nav.agents": "Agents",
  "nav.skills": "Skills",
  "nav.mcp": "MCP servers",
  "nav.browser": "Browser",
  "nav.settings": "Settings",
  "welcome.title": "What do you want to accomplish?",
  "welcome.body":
    "Open a local project, then describe the task naturally. HAWK uses Hawk K3 with live streaming and token metering.",
  "welcome.open": "Open local project",
  "welcome.quickStart": "Quick start suggestions",
  "welcome.review": "Review a project",
  "welcome.reviewHint": "Prepare a structure and issue analysis request",
  "welcome.reviewPrompt":
    "Review the open project, summarize its structure, and identify the most important issues with a verifiable plan.",
  "welcome.fix": "Fix an issue",
  "welcome.fixHint": "Prepare a diagnosis and fix request",
  "welcome.fixPrompt":
    "Analyze the issue in the open project and propose its root cause, a fix plan, and acceptance tests.",
  "welcome.provider": "Configure Hawk K3",
  "welcome.providerHint": "Check the key, model, and connection",
  "welcome.desktopOnly": "This feature is available in the desktop app.",
  "welcome.openError": "Unable to open the workspace.",
  "composer.placeholder": "Give HAWK a task…",
  "composer.projectPlaceholder": "Ask HAWK about {{project}}…",
  "composer.send": "Send",
  "composer.stop": "Stop",
  "composer.hint": "Enter to send · Shift+Enter for a new line",
  "composer.cancelled": "The Hawk K3 response was stopped.",
  "composer.queued": "Your follow-up was queued and will run next.",
  "composer.queuedLabel": "Queued",
  "message.copy": "Copy response",
  "message.copied": "Copied",
  "message.copyCode": "Copy code",
  "message.codeCopied": "Code copied",
  "conversation.jumpToBottom": "Jump to latest",
  "planning.questionsTitle": "Choose your planning answers",
  "planning.progress": "{{selected}} of {{total}} selected",
  "planning.submitAnswers": "Continue with these answers",
  "timing.thinking": "Thinking · {{time}}",
  "timing.responding": "Writing the response · {{time}}",
  "timing.completed": "Thought for {{thinking}} · Completed in {{total}}",
  "activity.title": "Agent activity",
  "activity.working": "Working on the project…",
  "activity.completedSteps": "Worked through {{count}} project steps",
  "activity.stoppedSteps": "Stopped after {{count}} project steps",
  "activity.interrupted":
    "This earlier turn ended before it could return a final response. Run it again to continue.",
  "activity.toolBudgetError":
    "The emergency loop guard stopped repeated agent actions before Hawk K3 returned a final summary.",
  "activity.list_files": "Scanned project files",
  "activity.project_graph_structure": "Recalled project structure",
  "activity.project_graph_query": "Searched project memory",
  "activity.read_file": "Read file",
  "activity.read_files": "Read project files",
  "activity.git_status": "Inspected Git changes",
  "activity.replace_in_file": "Edited file",
  "activity.write_file": "Wrote file",
  "activity.create_skill": "Created project skill",
  "activity.run_check": "Ran project check",
  "stages.understanding": "Understanding the request",
  "stages.inspecting": "Inspecting the project",
  "stages.planning": "Planning the approach",
  "stages.editing": "Editing files",
  "stages.verifying": "Verifying results",
  "stages.responding": "Writing the response",
  "stages.paused": "Paused — connection issue",
  "stages.failed": "Stopped with an error",
  "stages.completed": "Done",
  "resume.title": "Unfinished task found",
  "resume.body":
    "“{{task}}” was interrupted after {{steps}} steps · last activity {{time}}",
  "resume.continue": "Continue task",
  "resume.review": "Review state",
  "resume.cancel": "Dismiss",
  "resume.dismissed": "The saved task state was cleared.",
  "resume.progress": "Saved after {{steps}} completed steps",
  "resume.files": "{{count}} files changed",
  "composer.continue": "Continue response",
  "composer.offlineNotice":
    "Connection lost — HAWK paused the task and saved the state locally. Reconnect and continue the response.",
  "composer.offlineSend":
    "You are offline. Reconnect first so HAWK can reach Hawk K3.",
  "pending.title": "Instructions during the task",
  "pending.hint": "Applied as soon as the current step finishes",
  "pending.edit": "Edit instruction",
  "pending.delete": "Remove instruction",
  "pending.moveUp": "Move up",
  "pending.moveDown": "Move down",
  "offline.title": "Connection unavailable",
  "offline.body":
    "HAWK paused the active task. Everything is saved locally and will resume when you continue.",
  "slash.title": "Chat commands",
  "slash.review": "Review the active project with real tools",
  "slash.settings": "Open application settings",
  "slash.theme": "Switch the appearance theme",
  "slash.language": "Change the interface language",
  "slash.browser": "Open the internal browser",
  "slash.skills": "Open and manage skills",
  "slash.skill": "Enable a skill by name",
  "slash.mcp": "Open MCP servers",
  "slash.new": "Start a new conversation",
  "composer.add": "Add",
  "composer.files": "Files",
  "composer.filesDetail": "Attach images, code, or text documents",
  "composer.images": "Images",
  "composer.imagesDetail": "PNG, JPEG, WebP, GIF, or BMP",
  "composer.attachments": "Attached context",
  "composer.removeAttachment": "Remove",
  "composer.attachmentPrompt": "Analyze the attached context.",
  "composer.visionFallback":
    "This legacy endpoint rejected visual input, so this image request is using an alternative model.",
  "composer.visionNotSupported":
    "The current model (Hawk K3 Coder) does not support images. The image was attached but cannot be analyzed. Switch to a vision-capable provider for image analysis.",
  "composer.visionAnalyzing":
    "Images are being analyzed by Qwen-VL vision model, then Hawk K3 will respond.",
  "composer.longPasteAttached":
    "The long pasted text was attached as a TXT file.",
  "composer.plan": "Plan",
  "composer.planFirst": "Plan first and ask focused questions",
  "composer.startListening": "Start voice input",
  "composer.stopListening": "Stop voice input",
  "composer.listening": "Listening and transcribing…",
  "composer.voiceUnavailable":
    "Live speech recognition is not available in this WebView installation.",
  "composer.microphoneUnavailable": "No microphone input is available.",
  "composer.voiceError": "Voice input failed: {{error}}",
  "composer.voicePrivacy":
    "Microphone audio is processed by the Microsoft Edge speech service unless on-device recognition is enabled in the installed WebView runtime.",
  "composer.imagePasted":
    "Image from clipboard attached.",
  "composer.toolRejected":
    "The agent action was stopped by user approval.",
  "composer.confirmTitle": "Agent action requires approval",
  "composer.confirmTool": "The agent wants to run {{tool}}",
  "composer.approve": "Approve",
  "composer.reject": "Reject",
  "composer.skillRequests": "Requested skills",
  "composer.modelMenu": "Choose model",
  "composer.permissionMenu": "Choose permissions",
  "composer.permissionQuestion": "How should HAWK actions be approved?",
  "permissions.ask": "Ask for approval",
  "permissions.askDetail":
    "Ask before edits, commands, network, or external actions",
  "permissions.auto": "Approve safe actions",
  "permissions.autoDetail":
    "Proceed inside the workspace; ask for sensitive actions",
  "permissions.full": "Full access",
  "permissions.fullDetail":
    "Broad workspace and network access; critical actions still require confirmation",
  "models.quality": "Best quality",
  "models.balanced": "Balanced",
  "models.economy": "Fast and economical",
  "command.title": "Command palette",
  "command.search": "Search commands…",
  "command.commands": "Commands",
  "command.newTask": "Start a new task",
  "command.openWorkspace": "Open workspace",
  "workspace.title": "Workspace",
  "workspace.body": "Open a local project and inspect its real structure.",
  "workspace.refresh": "Refresh",
  "workspace.files": "Files",
  "workspace.directories": "Directories",
  "workspace.stack": "Detected stack",
  "workspace.empty": "No workspace is open yet.",
  "git.title": "Git status",
  "git.body": "A live read of the branch and project changes.",
  "git.empty": "Open a workspace to read Git status.",
  "git.clean": "Clean",
  "git.changed": "{{count}} changes",
  "git.noChanges": "No local changes.",
  "git.filesChanged": "{{count}} files changed",
  "git.reviewChanges": "Review changed files and lines",
  "agents.title": "Agents",
  "agents.body": "Choose the role that guides Hawk K3 in this conversation.",
  "agents.selected": "Selected",
  "agents.select": "Select",
  "agents.coordinator": "Coordinator",
  "agents.coordinatorHint": "Organizes the goal, plan, and next step",
  "agents.planner": "Planner",
  "agents.plannerHint": "Focuses on requirements and acceptance criteria",
  "agents.code": "Code agent",
  "agents.codeHint": "Focuses on implementation, files, and tests",
  "agents.review": "Reviewer",
  "agents.reviewHint": "Checks risks, regressions, and evidence",
  "skills.title": "Skills",
  "skills.body":
    "Enable specialized instructions that are added to agent context.",
  "skills.on": "Enabled",
  "skills.off": "Disabled",
  "skills.hawk-graph": "HAWK Graph memory",
  "skills.hawk-graphHint": "Recall indexed project structure and changed files",
  "skills.project-analysis": "Project analysis",
  "skills.project-analysisHint": "Explore structure and dependencies",
  "skills.git-review": "Git review",
  "skills.git-reviewHint": "Understand changes and merge risks",
  "skills.test-planning": "Test planning",
  "skills.test-planningHint": "Write executable acceptance tests",
  "skills.security-review": "Security review",
  "skills.security-reviewHint": "Spot secret and permission risks",
  "prompt.edit": "Edit",
  "prompt.save": "Save",
  "prompt.cancel": "Cancel",
  "settings.title": "Settings",
  "settings.body":
    "Control providers, appearance, permissions, language, and account connections.",
  "settings.general": "General",
  "settings.appearance": "Appearance",
  "settings.account": "Account",
  "settings.qwen": "AI provider — Hawk K3",
  "settings.apiKey": "API key",
  "settings.model": "Model",
  "settings.saveKey": "Save key",
  "settings.testConnection": "Test connection",
  "settings.deleteKey": "Delete key",
  "settings.connectedKey": "Key",
  "settings.noKey": "No saved key",
  "settings.configured": "Configured",
  "settings.required": "Required",
  "settings.keySaved": "Key saved securely.",
  "settings.keyDeleted": "Saved key deleted.",
  "settings.keySecurity":
    "The key never enters React, SQLite, or Local Storage. It stays in Windows Credential Manager and is sent only to official Alibaba domains.",
  "settings.language": "Interface language",
  "settings.languageHint":
    "Verified language packs used across menus and application copy",
  "settings.permission": "Default permissions",
  "settings.permissionHint":
    "Sensitive actions remain subject to the central permission policy.",
  "settings.theme": "Theme",
  "settings.themeHint":
    "Use the system appearance or choose light or dark mode.",
  "theme.system": "System",
  "theme.dark": "Dark",
  "theme.light": "Light",
  "account.menu": "Account menu",
  "account.local": "Local account",
  "account.localSession": "Local-only session",
  "account.google": "Set up Google sign-in",
  "account.googleSetup":
    "Google sign-in needs a registered HAWK OAuth desktop client before it can be enabled.",
  "account.googleTitle": "Google account",
  "account.googleBody":
    "Google OAuth cannot be shipped with a placeholder client ID. Register the HAWK desktop application, then add its client ID to enable real sign-in.",
  "account.notConfigured": "OAuth application not configured",
  "account.logout": "Sign out",
  "auth.secure": "Protected locally",
  "auth.welcome": "Welcome back",
  "auth.createTitle": "Create your HAWK account",
  "auth.loginBody": "Sign in before opening your engineering workspace.",
  "auth.registerBody":
    "Create a local account protected by Windows Credential Manager.",
  "auth.providers": "Social sign-in providers",
  "auth.google": "Sign in with Google",
  "auth.github": "Sign in with GitHub",
  "auth.facebook": "Sign in with Facebook",
  "auth.ready": "Ready for secure sign-in",
  "auth.secureBrowser": "Opens securely in your browser",
  "auth.notConfigured": "Coming after provider approval",
  "auth.oauthStatusFailed": "HAWK could not read the OAuth configuration.",
  "auth.localDivider": "or use a protected local account",
  "auth.signIn": "Sign in",
  "auth.createAccount": "Create account",
  "auth.name": "Display name",
  "auth.email": "Email",
  "auth.password": "Password",
  "auth.passwordStrength": "Password strength",
  "auth.strength.weak": "Weak",
  "auth.strength.medium": "Medium",
  "auth.strength.strong": "Strong",
  "auth.strength.excellent": "Excellent",
  "auth.rule.length": "12–128 characters",
  "auth.rule.case": "Uppercase and lowercase letters",
  "auth.rule.number": "At least one number",
  "auth.rule.symbol": "At least one symbol",
  "auth.rule.personal": "No spaces, common phrase, or email fragment",
  "auth.localSecurity":
    "Passwords are never stored directly. HAWK derives an Argon2id verifier and keeps it in Windows Credential Manager; five failed attempts trigger a temporary lock.",
  "language.verified": "Verified",
  "language.import": "Import pack",
  "language.imported": "{{language}} language pack installed.",
  "language.generate": "Generate with Hawk K3",
  "language.requiresQwen":
    "Configure the Hawk K3 provider first so HAWK can generate and validate this language pack.",
  "language.more":
    "Choose from the extended catalog. Arabic and English are reviewed; other languages are generated on demand with Hawk K3, validated, and stored locally. You can also import a reviewed BCP-47 pack.",
  "mcp.title": "MCP servers",
  "mcp.body":
    "The bundled workspace MCP connects automatically. Advanced local servers remain available when needed.",
  "mcp.builtinTitle": "HAWK Workspace MCP",
  "mcp.builtinConnecting": "Starting the bundled stdio server…",
  "mcp.active": "Active",
  "mcp.connecting": "Connecting",
  "mcp.run": "Run",
  "mcp.openWorkspace": "Open a workspace before running this tool.",
  "mcp.advanced": "Advanced: connect another local MCP server",
  "mcp.consent": "Explicit connection only",
  "mcp.consentDetail":
    "HAWK starts only the executable you choose. Tool descriptions are untrusted, and no tool is invoked from this screen.",
  "mcp.name": "Server name",
  "mcp.executable": "Executable",
  "mcp.choose": "Choose",
  "mcp.args": "Arguments",
  "mcp.argsHint": "One argument per line — no shell parsing",
  "mcp.connect": "Connect and discover",
  "mcp.noDescription": "No description supplied",
  "mcp.noTools": "The server connected but exposed no tools.",
  "browser.title": "Internal browser",
  "browser.address": "Website address",
  "browser.addressPlaceholder": "Search or enter an address",
  "browser.go": "Open",
  "browser.reload": "Reload page",
  "browser.close": "Close page",
  "browser.desktopOnly":
    "The secure internal WebView is available in the desktop app.",
  "browser.invalidUrl": "Enter a valid HTTP or HTTPS address.",
  "browser.openFailed": "The internal browser could not open the page.",
  "browser.openTimeout":
    "The internal browser timed out while creating the page.",
  "browser.openExample": "Open test page",
  "browser.opening": "Opening page…",
  "browser.emptyTitle": "Run and inspect a website inside HAWK",
  "browser.emptyBody":
    "Enter a local development URL or an HTTPS address. The site runs in an isolated child WebView without access to HAWK commands.",
} as const;

const ar: Record<keyof typeof en, string> = {
  "top.home": "الرئيسية",
  "top.workspace": "لا توجد مساحة عمل مفتوحة",
  "top.localWorkspace": "مساحة عمل محلية",
  "top.safe": "اسأل للتأكيد",
  "top.stop": "إيقاف الكل",
  "top.toggleSidebar": "إظهار أو إخفاء الشريط الجانبي",
  "top.nothingRunning": "لا توجد عملية قيد التشغيل.",
  "top.stopped": "تم إيقاف جميع المهام.",
  "top.running": "Hawk K3 يعمل الآن",
  "top.ready": "جاهز",
  "top.tokens": "رمز",
  "sidebar.newTask": "مهمة جديدة",
  "sidebar.search": "بحث أو أمر",
  "sidebar.navigation": "التنقل الرئيسي",
  "sidebar.projects": "المشاريع",
  "sidebar.addProject": "إضافة أو فتح مشروع",
  "sidebar.noProject": "لا يوجد مشروع مفتوح",
  "sidebar.noSessions": "لا توجد محادثات بعد",
  "sidebar.noSessionsHint": "افتح مشروعًا وابدأ أول مهمة.",
  "sidebar.workspaceChat": "محادثة المشروع",
  "sidebar.workspaceChatHint": "المحادثة الحالية مرتبطة بمشروع {{project}}.",
  "sidebar.generalChat": "دردشة عامة",
  "sidebar.generalChatHint": "محادثة خارج أي مشروع",
  "sidebar.projectChats": "محادثات المشروع",
  "sidebar.generalChats": "المحادثات",
  "sidebar.newChat": "دردشة جديدة",
  "sidebar.renameChat": "تغيير اسم الدردشة",
  "sidebar.deleteChat": "حذف الدردشة",
  "sidebar.chatOptions": "خيارات المحادثة",
  "sidebar.removeProject": "إزالة المشروع من HAWK",
  "sidebar.messageCount": "{{count}} رسالة",
  "sidebar.emptyChat": "لا توجد رسائل بعد",
  "nav.tasks": "المهام",
  "nav.workspaces": "مساحات العمل",
  "nav.git": "تغييرات Git",
  "nav.agents": "الوكلاء",
  "nav.skills": "المهارات",
  "nav.mcp": "خوادم MCP",
  "nav.browser": "المتصفح",
  "nav.settings": "الإعدادات",
  "welcome.title": "ماذا تريد أن تنجز؟",
  "welcome.body":
    "افتح مشروعًا محليًا، ثم صف المهمة بطريقتك. يستخدم HAWK نموذج Hawk K3 مع بث مباشر وقياس للرموز.",
  "welcome.open": "فتح مشروع محلي",
  "welcome.quickStart": "اقتراحات للبدء",
  "welcome.review": "مراجعة مشروع",
  "welcome.reviewHint": "جهّز طلبًا لتحليل البنية والمشكلات",
  "welcome.reviewPrompt":
    "راجع المشروع المفتوح، لخّص بنيته، وحدد أهم المشكلات مع خطة قابلة للتحقق.",
  "welcome.fix": "إصلاح مشكلة",
  "welcome.fixHint": "جهّز طلب تشخيص وإصلاح",
  "welcome.fixPrompt":
    "حلّل المشكلة في المشروع المفتوح واقترح سببها الجذري وخطة إصلاح واختبارات قبول.",
  "welcome.provider": "إعداد Hawk K3",
  "welcome.providerHint": "افحص المفتاح والنموذج والاتصال",
  "welcome.desktopOnly": "هذه الوظيفة متاحة داخل تطبيق سطح المكتب.",
  "welcome.openError": "تعذر فتح مساحة العمل.",
  "composer.placeholder": "اكتب مهمة إلى HAWK…",
  "composer.projectPlaceholder": "اسأل HAWK عن {{project}}…",
  "composer.send": "إرسال",
  "composer.stop": "إيقاف",
  "composer.hint": "Enter للإرسال · Shift+Enter لسطر جديد",
  "composer.cancelled": "تم إيقاف استجابة Hawk K3.",
  "composer.queued": "تمت إضافة توجيهك إلى الطابور وسيُنفذ بعد المهمة الحالية.",
  "composer.queuedLabel": "في الانتظار",
  "message.copy": "نسخ الرد",
  "message.copied": "تم النسخ",
  "message.copyCode": "نسخ الكود",
  "message.codeCopied": "تم نسخ الكود",
  "conversation.jumpToBottom": "الانتقال إلى آخر المحادثة",
  "planning.questionsTitle": "اختر إجابات التخطيط",
  "planning.progress": "تم اختيار {{selected}} من {{total}}",
  "planning.submitAnswers": "المتابعة بهذه الإجابات",
  "timing.thinking": "يفكر · {{time}}",
  "timing.responding": "يكتب الرد · {{time}}",
  "timing.completed": "فكّر لمدة {{thinking}} · اكتمل خلال {{total}}",
  "activity.title": "نشاط الوكيل",
  "activity.working": "يعمل على المشروع…",
  "activity.completedSteps": "اكتملت {{count}} خطوة في المشروع",
  "activity.stoppedSteps": "توقف بعد {{count}} خطوة في المشروع",
  "activity.interrupted":
    "انتهت هذه المحاولة السابقة قبل أن تعرض الرد النهائي. أعد تشغيلها للمتابعة.",
  "activity.toolBudgetError":
    "أوقف قاطع الطوارئ تكرار إجراءات الوكيل قبل أن يعرض Hawk K3 الملخص النهائي.",
  "activity.list_files": "فحص ملفات المشروع",
  "activity.project_graph_structure": "استدعاء هيكل المشروع المحفوظ",
  "activity.project_graph_query": "البحث في ذاكرة المشروع",
  "activity.read_file": "قراءة ملف",
  "activity.read_files": "قراءة ملفات المشروع",
  "activity.git_status": "فحص تغييرات Git",
  "activity.replace_in_file": "تعديل ملف",
  "activity.write_file": "كتابة ملف",
  "activity.create_skill": "إنشاء مهارة للمشروع",
  "activity.run_check": "تشغيل فحص المشروع",
  "stages.understanding": "فهم الطلب",
  "stages.inspecting": "فحص المشروع",
  "stages.planning": "تخطيط المنهج",
  "stages.editing": "تعديل الملفات",
  "stages.verifying": "التحقق من النتائج",
  "stages.responding": "كتابة الرد",
  "stages.paused": "متوقف — مشكلة اتصال",
  "stages.failed": "توقف مع خطأ",
  "stages.completed": "اكتمل",
  "resume.title": "تم العثور على مهمة غير مكتملة",
  "resume.body":
    "«{{task}}» توقفت بعد {{steps}} خطوة · آخر نشاط {{time}}",
  "resume.continue": "متابعة المهمة",
  "resume.review": "مراجعة الحالة",
  "resume.cancel": "إلغاء",
  "resume.dismissed": "تم مسح الحالة المحفوظة للمهمة.",
  "resume.progress": "حُفظت بعد {{steps}} خطوة مكتملة",
  "resume.files": "تم تعديل {{count}} ملفات",
  "composer.continue": "إكمال الرد",
  "composer.offlineNotice":
    "انقطع الاتصال — أوقف HAWK المهمة مؤقتًا وحفظ الحالة محليًا. أعد الاتصال ثم أكمل الرد.",
  "composer.offlineSend": "أنت غير متصل. أعد الاتصال أولًا ليصل HAWK إلى Hawk K3.",
  "pending.title": "تعليمات أثناء المهمة",
  "pending.hint": "تُطبَّق فور انتهاء الخطوة الحالية",
  "pending.edit": "تعديل التعليمات",
  "pending.delete": "حذف التعليمات",
  "pending.moveUp": "نقل للأعلى",
  "pending.moveDown": "نقل للأسفل",
  "offline.title": "الاتصال غير متاح",
  "offline.body":
    "أوقف HAWK المهمة النشطة مؤقتًا. كل شيء محفوظ محليًا وسيُستأنف عند المتابعة.",
  "slash.title": "أوامر المحادثة",
  "slash.review": "مراجعة المشروع الحالي بأدوات فعلية",
  "slash.settings": "فتح إعدادات التطبيق",
  "slash.theme": "تغيير الوضع الفاتح أو الداكن",
  "slash.language": "تغيير لغة الواجهة",
  "slash.browser": "فتح المتصفح الداخلي",
  "slash.skills": "فتح المهارات وإدارتها",
  "slash.skill": "تفعيل مهارة بالاسم",
  "slash.mcp": "فتح خوادم MCP",
  "slash.new": "بدء محادثة جديدة",
  "composer.add": "إضافة",
  "composer.files": "ملفات",
  "composer.filesDetail": "أرفق صورًا أو شيفرة أو مستندات نصية",
  "composer.images": "صور",
  "composer.imagesDetail": "PNG أو JPEG أو WebP أو GIF أو BMP",
  "composer.attachments": "السياق المرفق",
  "composer.removeAttachment": "إزالة",
  "composer.attachmentPrompt": "حلّل السياق المرفق.",
  "composer.visionFallback":
    "رفض هذا النطاق القديم الإدخال المرئي، لذلك يستخدم هذا الطلب نموذجًا بديلًا.",
  "composer.visionNotSupported":
    "النموذج الحالي (Hawk K3 Coder) لا يدعم الصور. تم إرفاق الصورة لكن لا يمكن تحليلها. انتقل إلى مزود يدعم الرؤية لتحليل الصور.",
  "composer.visionAnalyzing":
    "الصور قيد التحليل بواسطة نموذج الرؤية Qwen-VL، ثم سيجيب Hawk K3.",
  "composer.longPasteAttached": "أُرفق النص الطويل كملف TXT.",
  "composer.plan": "خطط أولًا",
  "composer.planFirst": "خطط أولًا واطرح أسئلة مركزة",
  "composer.startListening": "بدء الإدخال الصوتي",
  "composer.stopListening": "إيقاف الإدخال الصوتي",
  "composer.listening": "أستمع وأكتب مباشرة…",
  "composer.voiceUnavailable":
    "التعرف المباشر على الكلام غير متاح في تثبيت WebView الحالي.",
  "composer.microphoneUnavailable": "لا يتوفر إدخال من الميكروفون.",
  "composer.voiceError": "تعذر الإدخال الصوتي: {{error}}",
  "composer.voicePrivacy":
    "تُعالج بيانات الميكروفون عبر خدمة الكلام في Microsoft Edge ما لم يكن التعرف المحلي مفعّلًا في إصدار WebView المثبت.",
  "composer.imagePasted": "تمت إرفاق الصورة من الحافظة.",
  "composer.toolRejected": "تم إيقاف إجراء الوكيل بقرار المستخدم.",
  "composer.confirmTitle": "إجراء الوكيل يحتاج موافقة",
  "composer.confirmTool": "يريد الوكيل تشغيل {{tool}}",
  "composer.approve": "موافقة",
  "composer.reject": "رفض",
  "composer.skillRequests": "المهارات المطلوبة",
  "composer.modelMenu": "اختيار النموذج",
  "composer.permissionMenu": "اختيار الصلاحيات",
  "composer.permissionQuestion": "كيف تتم الموافقة على إجراءات HAWK؟",
  "permissions.ask": "اسأل للتأكيد",
  "permissions.askDetail":
    "اسأل قبل التعديل أو الأوامر أو الشبكة أو الإجراءات الخارجية",
  "permissions.auto": "وافق على الإجراءات الآمنة",
  "permissions.autoDetail": "تابع داخل المشروع واسأل عن الإجراءات الحساسة",
  "permissions.full": "وصول كامل",
  "permissions.fullDetail":
    "وصول واسع للمشروع والشبكة؛ تبقى الإجراءات الحرجة بحاجة لتأكيد",
  "models.quality": "أفضل جودة",
  "models.balanced": "متوازن",
  "models.economy": "سريع واقتصادي",
  "command.title": "لوحة الأوامر",
  "command.search": "ابحث عن أمر…",
  "command.commands": "الأوامر",
  "command.newTask": "بدء مهمة جديدة",
  "command.openWorkspace": "فتح مساحة عمل",
  "workspace.title": "مساحة العمل",
  "workspace.body": "افتح مشروعًا محليًا وافحص بنيته الفعلية.",
  "workspace.refresh": "تحديث",
  "workspace.files": "ملفات",
  "workspace.directories": "مجلدات",
  "workspace.stack": "التقنيات المكتشفة",
  "workspace.empty": "لم تفتح مساحة عمل بعد.",
  "git.title": "حالة Git",
  "git.body": "قراءة مباشرة للفرع وتغييرات المشروع.",
  "git.empty": "افتح مساحة عمل لقراءة حالة Git.",
  "git.clean": "نظيف",
  "git.changed": "{{count}} تغييرات",
  "git.noChanges": "لا توجد تغييرات محلية.",
  "git.filesChanged": "تم تعديل {{count}} ملفات",
  "git.reviewChanges": "مراجعة الملفات والأسطر المعدلة",
  "agents.title": "الوكلاء",
  "agents.body": "اختر الدور الذي يوجّه Hawk K3 في هذه المحادثة.",
  "agents.selected": "محدد",
  "agents.select": "اختيار",
  "agents.coordinator": "المنسق",
  "agents.coordinatorHint": "ينظم الهدف والخطة والخطوة التالية",
  "agents.planner": "المخطط",
  "agents.plannerHint": "يركز على المتطلبات وشروط القبول",
  "agents.code": "وكيل الشيفرة",
  "agents.codeHint": "يركز على التنفيذ والملفات والاختبارات",
  "agents.review": "المراجع",
  "agents.reviewHint": "يفحص المخاطر والانحدارات والأدلة",
  "skills.title": "المهارات",
  "skills.body": "فعّل تعليمات متخصصة تضاف إلى سياق الوكيل.",
  "skills.on": "مفعلة",
  "skills.off": "معطلة",
  "skills.hawk-graph": "ذاكرة HAWK Graph",
  "skills.hawk-graphHint": "استرجاع بنية المشروع والفروقات المفهرسة",
  "skills.project-analysis": "تحليل المشاريع",
  "skills.project-analysisHint": "استكشاف البنية والتبعيات",
  "skills.git-review": "مراجعة Git",
  "skills.git-reviewHint": "فهم التغييرات ومخاطر الدمج",
  "skills.test-planning": "تخطيط الاختبارات",
  "skills.test-planningHint": "كتابة اختبارات قبول قابلة للتنفيذ",
  "skills.security-review": "المراجعة الأمنية",
  "skills.security-reviewHint": "رصد مخاطر الأسرار والصلاحيات",
  "prompt.edit": "تعديل",
  "prompt.save": "حفظ",
  "prompt.cancel": "إلغاء",
  "settings.title": "الإعدادات",
  "settings.body": "تحكم في المزود والمظهر والصلاحيات واللغة واتصالات الحساب.",
  "settings.general": "عام",
  "settings.appearance": "المظهر",
  "settings.account": "الحساب",
  "settings.qwen": "مزود الذكاء الاصطناعي — Hawk K3",
  "settings.apiKey": "مفتاح API",
  "settings.model": "النموذج",
  "settings.saveKey": "حفظ المفتاح",
  "settings.testConnection": "اختبار الاتصال",
  "settings.deleteKey": "حذف المفتاح",
  "settings.connectedKey": "المفتاح",
  "settings.noKey": "لا يوجد مفتاح محفوظ",
  "settings.configured": "مهيأ",
  "settings.required": "مطلوب",
  "settings.keySaved": "حُفظ المفتاح بأمان.",
  "settings.keyDeleted": "حُذف المفتاح المحفوظ.",
  "settings.keySecurity":
    "لا يدخل المفتاح إلى React أو SQLite أو Local Storage؛ يُحفظ في Windows Credential Manager ويرسل إلى نطاقات Alibaba الرسمية فقط.",
  "settings.language": "لغة الواجهة",
  "settings.languageHint": "حزم لغات مراجعة للقوائم ونصوص التطبيق",
  "settings.permission": "الصلاحيات الافتراضية",
  "settings.permissionHint":
    "تبقى الإجراءات الحساسة خاضعة لسياسة الصلاحيات المركزية.",
  "settings.theme": "المظهر",
  "settings.themeHint": "استخدم مظهر النظام أو اختر الوضع الفاتح أو الداكن.",
  "theme.system": "النظام",
  "theme.dark": "داكن",
  "theme.light": "فاتح",
  "account.menu": "قائمة الحساب",
  "account.local": "حساب محلي",
  "account.localSession": "جلسة محلية فقط",
  "account.google": "إعداد تسجيل Google",
  "account.googleSetup":
    "يتطلب تسجيل Google تطبيق OAuth مكتبيًا مسجلًا باسم HAWK قبل تفعيله.",
  "account.googleTitle": "حساب Google",
  "account.googleBody":
    "لا يمكن شحن Google OAuth بمعرّف عميل وهمي. سجّل تطبيق HAWK المكتبي ثم أضف Client ID لتفعيل تسجيل الدخول الحقيقي.",
  "account.notConfigured": "تطبيق OAuth غير مهيأ",
  "account.logout": "تسجيل الخروج",
  "auth.secure": "محمي محليًا",
  "auth.welcome": "مرحبًا بعودتك",
  "auth.createTitle": "أنشئ حساب HAWK",
  "auth.loginBody": "سجّل الدخول قبل فتح مساحة العمل الهندسية.",
  "auth.registerBody":
    "أنشئ حسابًا محليًا محميًا داخل Windows Credential Manager.",
  "auth.providers": "مزودو تسجيل الدخول الاجتماعي",
  "auth.google": "تسجيل الدخول باستخدام Google",
  "auth.github": "تسجيل الدخول باستخدام GitHub",
  "auth.facebook": "تسجيل الدخول باستخدام Facebook",
  "auth.ready": "جاهز لتسجيل الدخول الآمن",
  "auth.secureBrowser": "يفتح بأمان داخل متصفحك",
  "auth.notConfigured": "قريبًا بعد اعتماد المزوّد",
  "auth.oauthStatusFailed": "تعذر على HAWK قراءة إعداد OAuth.",
  "auth.localDivider": "أو استخدم حسابًا محليًا محميًا",
  "auth.signIn": "تسجيل الدخول",
  "auth.createAccount": "إنشاء حساب",
  "auth.name": "اسم العرض",
  "auth.email": "البريد الإلكتروني",
  "auth.password": "كلمة المرور",
  "auth.passwordStrength": "قوة كلمة المرور",
  "auth.strength.weak": "ضعيفة",
  "auth.strength.medium": "متوسطة",
  "auth.strength.strong": "قوية",
  "auth.strength.excellent": "ممتازة",
  "auth.rule.length": "من 12 إلى 128 حرفًا",
  "auth.rule.case": "أحرف كبيرة وصغيرة",
  "auth.rule.number": "رقم واحد على الأقل",
  "auth.rule.symbol": "رمز خاص واحد على الأقل",
  "auth.rule.personal": "دون مسافات أو عبارة شائعة أو جزء من البريد",
  "auth.localSecurity":
    "لا تُحفظ كلمة المرور مباشرة. يشتق HAWK مدقق Argon2id ويحفظه داخل Windows Credential Manager؛ خمس محاولات فاشلة تؤدي إلى قفل مؤقت.",
  "language.verified": "مراجعة",
  "language.import": "استيراد حزمة",
  "language.imported": "تم تثبيت حزمة لغة {{language}}.",
  "language.generate": "إنشاء عبر Hawk K3",
  "language.requiresQwen":
    "اضبط مزود Hawk K3 أولًا لكي ينشئ HAWK حزمة اللغة ويتحقق منها.",
  "language.more":
    "اختر من القائمة الموسعة. العربية والإنجليزية مراجعتان؛ تُنشأ اللغات الأخرى عند الطلب بواسطة Hawk K3 ثم تتحقق وتحفظ محليًا. ويمكن أيضًا استيراد حزمة BCP-47 مراجعة.",
  "mcp.title": "خوادم MCP",
  "mcp.body":
    "يتصل MCP المدمج لمساحة العمل تلقائيًا، وتبقى الخوادم المحلية المتقدمة متاحة عند الحاجة.",
  "mcp.builtinTitle": "HAWK Workspace MCP",
  "mcp.builtinConnecting": "جارٍ تشغيل خادم stdio المدمج…",
  "mcp.active": "فعّال",
  "mcp.connecting": "جارٍ الاتصال",
  "mcp.run": "تشغيل",
  "mcp.openWorkspace": "افتح مساحة عمل قبل تشغيل هذه الأداة.",
  "mcp.advanced": "متقدم: توصيل خادم MCP محلي آخر",
  "mcp.consent": "اتصال بموافقة صريحة فقط",
  "mcp.consentDetail":
    "يشغّل HAWK الملف التنفيذي الذي تختاره فقط. أوصاف الأدوات غير موثوقة، ولا تُستدعى أي أداة من هذه الشاشة.",
  "mcp.name": "اسم الخادم",
  "mcp.executable": "الملف التنفيذي",
  "mcp.choose": "اختيار",
  "mcp.args": "الوسائط",
  "mcp.argsHint": "وسيط واحد في كل سطر — دون تحليل Shell",
  "mcp.connect": "اتصال واكتشاف",
  "mcp.noDescription": "لم يرسل الخادم وصفًا",
  "mcp.noTools": "تم الاتصال لكن الخادم لم يعرض أدوات.",
  "browser.title": "المتصفح الداخلي",
  "browser.address": "عنوان الموقع",
  "browser.addressPlaceholder": "ابحث أو أدخل عنوانًا",
  "browser.go": "فتح",
  "browser.reload": "إعادة تحميل الصفحة",
  "browser.close": "إغلاق الصفحة",
  "browser.desktopOnly": "يتوفر WebView الداخلي الآمن في تطبيق سطح المكتب.",
  "browser.invalidUrl": "أدخل عنوان HTTP أو HTTPS صالحًا.",
  "browser.openFailed": "تعذر على المتصفح الداخلي فتح الصفحة.",
  "browser.openTimeout": "انتهت مهلة إنشاء الصفحة داخل المتصفح.",
  "browser.openExample": "فتح صفحة اختبار",
  "browser.opening": "جارٍ فتح الصفحة…",
  "browser.emptyTitle": "شغّل موقعًا وافحصه داخل HAWK",
  "browser.emptyBody":
    "أدخل عنوان تطوير محليًا أو عنوان HTTPS. يعمل الموقع داخل WebView فرعي معزول دون وصول إلى أوامر HAWK.",
};

const savedLanguage = window.localStorage.getItem("hawk.language.v1");
const languageDirections = new Map<string, "ltr" | "rtl">([
  ["ar", "rtl"],
  ["en", "ltr"],
]);
const installedLanguageNames = new Map<string, string>();
const loadedLanguagePacks: HawkLanguagePack[] = [];

const LANGUAGE_CATALOG: ReadonlyArray<{
  locale: string;
  name: string;
  direction: "ltr" | "rtl";
}> = [
  { locale: "ar", name: "العربية", direction: "rtl" },
  { locale: "en", name: "English", direction: "ltr" },
  { locale: "fr", name: "Français", direction: "ltr" },
  { locale: "es", name: "Español", direction: "ltr" },
  { locale: "de", name: "Deutsch", direction: "ltr" },
  { locale: "it", name: "Italiano", direction: "ltr" },
  { locale: "pt-BR", name: "Português (Brasil)", direction: "ltr" },
  { locale: "pt-PT", name: "Português (Portugal)", direction: "ltr" },
  { locale: "nl", name: "Nederlands", direction: "ltr" },
  { locale: "sv", name: "Svenska", direction: "ltr" },
  { locale: "no", name: "Norsk", direction: "ltr" },
  { locale: "da", name: "Dansk", direction: "ltr" },
  { locale: "fi", name: "Suomi", direction: "ltr" },
  { locale: "is", name: "Íslenska", direction: "ltr" },
  { locale: "pl", name: "Polski", direction: "ltr" },
  { locale: "cs", name: "Čeština", direction: "ltr" },
  { locale: "sk", name: "Slovenčina", direction: "ltr" },
  { locale: "sl", name: "Slovenščina", direction: "ltr" },
  { locale: "hr", name: "Hrvatski", direction: "ltr" },
  { locale: "sr", name: "Српски", direction: "ltr" },
  { locale: "bs", name: "Bosanski", direction: "ltr" },
  { locale: "ro", name: "Română", direction: "ltr" },
  { locale: "hu", name: "Magyar", direction: "ltr" },
  { locale: "el", name: "Ελληνικά", direction: "ltr" },
  { locale: "bg", name: "Български", direction: "ltr" },
  { locale: "uk", name: "Українська", direction: "ltr" },
  { locale: "ru", name: "Русский", direction: "ltr" },
  { locale: "tr", name: "Türkçe", direction: "ltr" },
  { locale: "he", name: "עברית", direction: "rtl" },
  { locale: "fa", name: "فارسی", direction: "rtl" },
  { locale: "ur", name: "اردو", direction: "rtl" },
  { locale: "hi", name: "हिन्दी", direction: "ltr" },
  { locale: "bn", name: "বাংলা", direction: "ltr" },
  { locale: "pa", name: "ਪੰਜਾਬੀ", direction: "ltr" },
  { locale: "gu", name: "ગુજરાતી", direction: "ltr" },
  { locale: "mr", name: "मराठी", direction: "ltr" },
  { locale: "ta", name: "தமிழ்", direction: "ltr" },
  { locale: "te", name: "తెలుగు", direction: "ltr" },
  { locale: "kn", name: "ಕನ್ನಡ", direction: "ltr" },
  { locale: "ml", name: "മലയാളം", direction: "ltr" },
  { locale: "th", name: "ไทย", direction: "ltr" },
  { locale: "vi", name: "Tiếng Việt", direction: "ltr" },
  { locale: "id", name: "Bahasa Indonesia", direction: "ltr" },
  { locale: "ms", name: "Bahasa Melayu", direction: "ltr" },
  { locale: "fil", name: "Filipino", direction: "ltr" },
  { locale: "zh-CN", name: "简体中文", direction: "ltr" },
  { locale: "zh-TW", name: "繁體中文", direction: "ltr" },
  { locale: "ja", name: "日本語", direction: "ltr" },
  { locale: "ko", name: "한국어", direction: "ltr" },
  { locale: "sw", name: "Kiswahili", direction: "ltr" },
  { locale: "am", name: "አማርኛ", direction: "ltr" },
  { locale: "af", name: "Afrikaans", direction: "ltr" },
  { locale: "sq", name: "Shqip", direction: "ltr" },
  { locale: "az", name: "Azərbaycanca", direction: "ltr" },
  { locale: "ka", name: "ქართული", direction: "ltr" },
  { locale: "hy", name: "Հայերեն", direction: "ltr" },
  { locale: "kk", name: "Қазақша", direction: "ltr" },
  { locale: "uz", name: "O‘zbekcha", direction: "ltr" },
];

for (const language of LANGUAGE_CATALOG)
  languageDirections.set(language.locale, language.direction);

export interface HawkLanguagePack {
  locale: string;
  name: string;
  direction: "ltr" | "rtl";
  translations: Record<string, string>;
}

function validPack(pack: HawkLanguagePack): boolean {
  return (
    /^[a-z]{2,3}(?:-[A-Z]{2})?$/.test(pack.locale) &&
    pack.name.trim().length > 0 &&
    pack.name.length <= 80 &&
    Object.keys(pack.translations).length > 0 &&
    Object.keys(pack.translations).length <= 500 &&
    Object.values(pack.translations).every(
      (value) => typeof value === "string" && value.length <= 4_000,
    )
  );
}

function registerPack(pack: HawkLanguagePack, persist: boolean): void {
  if (!validPack(pack))
    throw new Error(
      "The HAWK language pack is invalid or exceeds the safe limits.",
    );
  if (i18n.isInitialized)
    i18n.addResourceBundle(
      pack.locale,
      "translation",
      pack.translations,
      true,
      true,
    );
  else loadedLanguagePacks.push(pack);
  languageDirections.set(pack.locale, pack.direction);
  installedLanguageNames.set(pack.locale, pack.name);
  if (persist)
    window.localStorage.setItem(
      `hawk.languagePack.v1.${pack.locale}`,
      JSON.stringify(pack),
    );
}

for (let index = 0; index < window.localStorage.length; index += 1) {
  const key = window.localStorage.key(index);
  if (!key?.startsWith("hawk.languagePack.v1.")) continue;
  try {
    registerPack(
      JSON.parse(window.localStorage.getItem(key) ?? "") as HawkLanguagePack,
      false,
    );
  } catch {
    window.localStorage.removeItem(key);
  }
}

const initialLanguage =
  savedLanguage &&
  (savedLanguage === "ar" ||
    savedLanguage === "en" ||
    installedLanguageNames.has(savedLanguage))
    ? savedLanguage
    : "ar";

void i18n.use(initReactI18next).init({
  resources: { ar: { translation: ar }, en: { translation: en } },
  lng: initialLanguage,
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

for (const pack of loadedLanguagePacks)
  i18n.addResourceBundle(
    pack.locale,
    "translation",
    pack.translations,
    true,
    true,
  );

document.documentElement.lang = initialLanguage;
document.documentElement.dir = initialLanguage === "ar" ? "rtl" : "ltr";

i18n.on("languageChanged", (language) => {
  window.localStorage.setItem("hawk.language.v1", language);
  document.documentElement.lang = language;
  document.documentElement.dir = languageDirections.get(language) ?? "ltr";
});

export function installLanguagePack(pack: HawkLanguagePack): void {
  registerPack(pack, true);
}

export function availableLanguages(): Array<{
  locale: string;
  name: string;
  verified: boolean;
  installed: boolean;
  direction: "ltr" | "rtl";
}> {
  const catalogLocales = new Set(LANGUAGE_CATALOG.map((item) => item.locale));
  return [
    ...LANGUAGE_CATALOG.map((language) => ({
      ...language,
      verified: language.locale === "ar" || language.locale === "en",
      installed:
        language.locale === "ar" ||
        language.locale === "en" ||
        installedLanguageNames.has(language.locale),
    })),
    ...[...installedLanguageNames]
      .filter(([locale]) => !catalogLocales.has(locale))
      .map(([locale, name]) => ({
        locale,
        name,
        direction: languageDirections.get(locale) ?? "ltr",
        verified: false,
        installed: true,
      })),
  ];
}

export function englishSourceTranslations(): Record<string, string> {
  return { ...en };
}

export default i18n;
