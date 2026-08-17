import {
  ArrowRight,
  Check,
  LoaderCircle,
  LockKeyhole,
  Mail,
  ShieldCheck,
  UserRound,
} from "lucide-react";
import { type FormEvent, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  getOAuthProviderStatuses,
  loginLocalAccount,
  loginWithGoogle,
  registerLocalAccount,
} from "../lib/ipc";
import { useWorkbenchStore } from "../store/workbench";
import { HawkBrand } from "./HawkBrand";
import { FacebookIcon, GitHubIcon, GoogleIcon } from "./ProviderIcons";

type AuthMode = "login" | "register";

export function AuthView() {
  const { t } = useTranslation();
  const completeAuthentication = useWorkbenchStore(
    (state) => state.completeAuthentication,
  );
  const [mode, setMode] = useState<AuthMode>("login");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [socialBusy, setSocialBusy] = useState<"google" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [providers, setProviders] = useState<Record<string, boolean>>({
    google: false,
    github: false,
    facebook: false,
  });
  const strength = useMemo(
    () => passwordStrength(password, email),
    [email, password],
  );

  useEffect(() => {
    let active = true;
    void getOAuthProviderStatuses()
      .then((statuses) => {
        if (!active) return;
        setProviders(
          Object.fromEntries(
            statuses.map((status) => [status.provider, status.configured]),
          ),
        );
      })
      .catch(() => {
        if (active) setError(t("auth.oauthStatusFailed"));
      });
    return () => {
      active = false;
    };
  }, [t]);

  const changeMode = (next: AuthMode) => {
    setMode(next);
    setError(null);
    setPassword("");
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const profile =
        mode === "register"
          ? await registerLocalAccount({ name, email, password })
          : await loginLocalAccount({ email, password });
      completeAuthentication(profile);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const signInWithGoogle = async () => {
    if (!providers.google) return;
    setSocialBusy("google");
    setError(null);
    try {
      completeAuthentication(await loginWithGoogle());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSocialBusy(null);
    }
  };

  return (
    <main className="auth-shell">
      <div className="auth-ambient auth-ambient--one" />
      <div className="auth-ambient auth-ambient--two" />
      <section className="auth-card" aria-labelledby="auth-title">
        <header className="auth-card__header">
          <HawkBrand />
          <span className="auth-security-badge">
            <ShieldCheck size={14} /> {t("auth.secure")}
          </span>
        </header>
        <div className="auth-card__intro">
          <span>HAWK STUDIO / SECURE ACCESS</span>
          <h1 id="auth-title">
            {mode === "login" ? t("auth.welcome") : t("auth.createTitle")}
          </h1>
          <p>
            {mode === "login" ? t("auth.loginBody") : t("auth.registerBody")}
          </p>
        </div>

        <div className="auth-providers" aria-label={t("auth.providers")}>
          <button
            type="button"
            onClick={() => void signInWithGoogle()}
            disabled={!providers.google || socialBusy !== null}
            aria-label={t("auth.google")}
            title={providers.google ? t("auth.ready") : t("auth.notConfigured")}
          >
            <span className="auth-provider-icon auth-provider-icon--google">
              {socialBusy === "google" ? (
                <LoaderCircle className="spin" size={19} />
              ) : (
                <GoogleIcon width={20} height={20} />
              )}
            </span>
            <span>{t("auth.google")}</span>
            <small>
              {providers.google
                ? t("auth.secureBrowser")
                : t("auth.notConfigured")}
            </small>
          </button>
          <button
            type="button"
            disabled={!providers.github}
            aria-label={t("auth.github")}
            title={t("auth.notConfigured")}
          >
            <span className="auth-provider-icon">
              <GitHubIcon width={20} height={20} />
            </span>
            <span>{t("auth.github")}</span>
            <small>{t("auth.notConfigured")}</small>
          </button>
          <button
            type="button"
            disabled={!providers.facebook}
            aria-label={t("auth.facebook")}
            title={t("auth.notConfigured")}
          >
            <span className="auth-provider-icon">
              <FacebookIcon width={20} height={20} />
            </span>
            <span>{t("auth.facebook")}</span>
            <small>{t("auth.notConfigured")}</small>
          </button>
        </div>

        <div className="auth-divider">
          <span>{t("auth.localDivider")}</span>
        </div>

        <div className="auth-tabs" data-mode={mode} role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={mode === "login"}
            data-active={mode === "login"}
            onClick={() => changeMode("login")}
          >
            {t("auth.signIn")}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mode === "register"}
            data-active={mode === "register"}
            onClick={() => changeMode("register")}
          >
            {t("auth.createAccount")}
          </button>
        </div>

        <form
          className="auth-form"
          data-mode={mode}
          key={mode}
          onSubmit={(event) => void submit(event)}
        >
          {mode === "register" ? (
            <label>
              <span>{t("auth.name")}</span>
              <div className="auth-input">
                <UserRound size={16} />
                <input
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  autoComplete="name"
                  minLength={2}
                  maxLength={80}
                  required
                />
              </div>
            </label>
          ) : null}
          <label>
            <span>{t("auth.email")}</span>
            <div className="auth-input">
              <Mail size={16} />
              <input
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                autoComplete="email"
                required
              />
            </div>
          </label>
          <label>
            <span>{t("auth.password")}</span>
            <div className="auth-input">
              <LockKeyhole size={16} />
              <input
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                autoComplete={
                  mode === "register" ? "new-password" : "current-password"
                }
                minLength={mode === "register" ? 12 : undefined}
                maxLength={128}
                required
              />
            </div>
          </label>

          {mode === "register" ? (
            <div className="password-strength" data-score={strength.score}>
              <div className="password-strength__label">
                <span>{t("auth.passwordStrength")}</span>
                <strong>{t(`auth.strength.${strength.label}`)}</strong>
              </div>
              <div className="password-strength__bars" aria-hidden="true">
                {[1, 2, 3, 4, 5].map((segment) => (
                  <i key={segment} data-filled={strength.score >= segment} />
                ))}
              </div>
              <ul>
                {strength.rules.map((rule) => (
                  <li key={rule.key} data-met={rule.met}>
                    <Check size={12} /> {t(`auth.rule.${rule.key}`)}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          {error ? (
            <p className="auth-error" role="alert">
              {error}
            </p>
          ) : null}

          <button
            className="auth-submit"
            type="submit"
            disabled={
              busy ||
              !email.trim() ||
              !password ||
              (mode === "register" && (!name.trim() || strength.score < 5))
            }
          >
            {busy ? <LoaderCircle className="spin" size={17} /> : null}
            <span>
              {mode === "login" ? t("auth.signIn") : t("auth.createAccount")}
            </span>
            <ArrowRight size={16} />
          </button>
        </form>
        <p className="auth-footnote">{t("auth.localSecurity")}</p>
      </section>
    </main>
  );
}

function passwordStrength(password: string, email: string) {
  const lower = password.toLowerCase();
  const local = email.toLowerCase().split("@")[0] ?? "";
  const rules = [
    { key: "length", met: password.length >= 12 && password.length <= 128 },
    {
      key: "case",
      met: /[a-z]/.test(password) && /[A-Z]/.test(password),
    },
    { key: "number", met: /\d/.test(password) },
    { key: "symbol", met: /[^\p{L}\p{N}\s]/u.test(password) },
    {
      key: "personal",
      met:
        !/\s/.test(password) &&
        !["password", "password123", "123456789", "qwerty123"].includes(
          lower,
        ) &&
        !(local.length >= 4 && lower.includes(local)),
    },
  ];
  const score = rules.filter((rule) => rule.met).length;
  const label =
    score <= 1
      ? "weak"
      : score <= 3
        ? "medium"
        : score === 4
          ? "strong"
          : "excellent";
  return { label, rules, score };
}
