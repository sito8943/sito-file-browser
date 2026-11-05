import { useOnboarding } from "@/features/onboarding";
import { t } from "@/lang";
import { USAGE_EVENT, useUsage } from "@/shared/usage";

import { useSettings } from "../../../providers/SettingsProvider";
import SettingsButton from "../SettingsButton";

// "Open guide" button: closes Settings (two stacked modals would fight for the MODAL scope) and
// reopens the welcome guide from its first page. Binds to no AppSettings key. Takes no props (the
// schema renders it with CustomControlProps, which it ignores).
const OnboardingReplayControl = () => {
  const { close } = useSettings();
  const { open } = useOnboarding();
  // Counted here rather than in the provider's open, which the first-launch auto-open also calls.
  const { track } = useUsage();
  const onClick = () => {
    track(USAGE_EVENT.APP.ONBOARDING_OPEN);
    close();
    open();
  };
  return (
    <SettingsButton onClick={onClick}>
      {t.settings.onboardingReplayButton}
    </SettingsButton>
  );
};

export default OnboardingReplayControl;
