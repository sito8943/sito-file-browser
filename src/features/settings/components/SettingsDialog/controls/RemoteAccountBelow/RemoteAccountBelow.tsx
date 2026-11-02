import { useState, type FormEvent } from "react";

import TextInput from "@/shared/components/elements/TextInput";
import PasswordInput from "@/shared/components/patterns/PasswordInput";
import { useRemote } from "@/features/remote";
import { t } from "@/lang";

import SettingsButton from "../../SettingsButton";
import { stateLabel } from "./utils";

// Below the Sito account row: who is signed in and the service state, or the sign-in form. The
// password only lives in this form's state until submitted; the backend keeps a refresh token in
// the OS keychain, never the password.
const RemoteAccountBelow = () => {
  const { account, status, signIn } = useRemote();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);

  if (account === undefined) return null;

  if (account) {
    return (
      <div className="settings_row_below">
        <span className="settings_row_hint">
          {t.remoteControl.signedInAs(account.email, stateLabel(status.state))}
        </span>
      </div>
    );
  }

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!email.trim() || !password) return;
    setBusy(true);
    // Keep the email for a retry; the password is cleared either way.
    if (await signIn(email.trim(), password)) setEmail("");
    setPassword("");
    setBusy(false);
  };

  return (
    <form className="settings_remote_signin" onSubmit={(e) => void submit(e)}>
      <span className="settings_row_hint">{t.remoteControl.signInHint}</span>
      <TextInput
        className="settings_input"
        type="email"
        value={email}
        placeholder={t.remoteControl.email}
        aria-label={t.remoteControl.email}
        onChange={(event) => setEmail(event.target.value)}
        disabled={busy}
        spellCheck={false}
        // eslint-disable-next-line i18next/no-literal-string -- HTML input attribute value
        autoCapitalize="off"
        // eslint-disable-next-line i18next/no-literal-string -- HTML input attribute value
        autoComplete="username"
      />
      <PasswordInput
        className="settings_input"
        value={password}
        placeholder={t.remoteControl.password}
        aria-label={t.remoteControl.password}
        onChange={(event) => setPassword(event.target.value)}
        disabled={busy}
        // eslint-disable-next-line i18next/no-literal-string -- HTML input attribute value
        autoComplete="current-password"
      />
      <SettingsButton
        type="submit"
        disabled={busy || !email.trim() || !password}
      >
        {t.remoteControl.signIn}
      </SettingsButton>
    </form>
  );
};

export default RemoteAccountBelow;
