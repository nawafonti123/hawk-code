# HAWK Code

**مركز قيادة هندسي بالذكاء الاصطناعي** من HAWK Studio.

يمثل هذا المستودع أساس المرحلة الأولى: تطبيق Tauri 2، واجهة React/TypeScript
صارمة، حد IPC مُرقّم الإصدار، قاعدة SQLite مع migrations، اختيار Workspace،
والنسخة الأولى من نظام تصميم HAWK.

## المتطلبات

- Node.js 22.12 أو أحدث
- pnpm 10
- Rust stable وCargo
- Windows WebView2 Runtime

محرك التحكم في Windows سيحتاج .NET LTS في مرحلة لاحقة، ولا يمنع تشغيل هذه
المرحلة.

## التشغيل

```powershell
pnpm install
pnpm dev:web
pnpm dev:desktop
```

## التحقق

```powershell
pnpm typecheck
pnpm lint
pnpm test
pnpm build
```
