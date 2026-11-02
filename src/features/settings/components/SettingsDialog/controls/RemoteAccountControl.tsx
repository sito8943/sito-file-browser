import { useRemote } from "@/features/remote";
import { t } from "@/lang";

import SettingsButton from "../SettingsButton";

// Sign-out button for the Sito account row; nothing while signed out (the form renders below).
// Takes no props (the schema renders it with CustomControlProps, which it ignores).
const RemoteAccountControl = () => {
  const { account, signOut } = useRemote();
  if (!account) return null;
  return (
    <SettingsButton onClick={() => void signOut()}>
      {t.remoteControl.signOut}
    </SettingsButton>
  );
};

export default RemoteAccountControl;
