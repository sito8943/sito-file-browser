import { useEffect, useRef, useState } from "react";

import Dialog from "@/shared/components/patterns/Dialog";
import DialogHeader from "@/shared/components/patterns/DialogHeader";
import DialogActions from "@/shared/components/patterns/DialogActions";
import Button from "@/shared/components/elements/Button";
import { classNames } from "@/shared/utils";
import { useSettings } from "@/features/settings";
import { t } from "@/lang";

import "@/styles/components/OnboardingDialog.css";

import { useOnboarding } from "../../providers/OnboardingProvider";
import { visibleSteps } from "../../steps";
import { ONBOARDING_TITLE_ID } from "./constants";
import type { OnboardingDialogProps } from "./types";

// The welcome guide: a paged modal driven by the ONBOARDING_STEPS registry. Opens itself on launch
// (once settings are hydrated) while `showOnboarding` is on and the guide hasn't been seen; any
// exit — Finish, Skip, Escape, backdrop — marks it seen so it doesn't nag on the next launch.
// Mounted inside SettingsProvider because its step bodies reuse the settings controls.
const OnboardingDialog = ({ settingsReady }: OnboardingDialogProps) => {
  const { visible, open, close } = useOnboarding();
  const { settings, update } = useSettings();
  const [index, setIndex] = useState(0);

  const steps = visibleSteps();
  const step = steps[Math.min(index, steps.length - 1)];
  const last = index >= steps.length - 1;

  // Auto-open on launch, once per session. Gated on hydration so the defaults (showOnboarding:
  // true) don't fire the guide before settings.toml has been read; the ref keeps a later toggle of
  // showOnboarding in Settings from popping the guide over the settings dialog.
  const autoOpened = useRef(false);
  useEffect(() => {
    if (!settingsReady || autoOpened.current) return;
    autoOpened.current = true;
    // Fresh installs only (no version recorded yet): after an update the what's-new toast takes
    // over, so the guide doesn't pile on top of it.
    if (
      settings.showOnboarding &&
      !settings.onboardingSeen &&
      settings.lastSeenVersion === ""
    )
      open();
  }, [
    settingsReady,
    settings.showOnboarding,
    settings.onboardingSeen,
    settings.lastSeenVersion,
    open,
  ]);

  // Start from the first page each time the guide opens.
  const [wasVisible, setWasVisible] = useState(visible);
  if (visible !== wasVisible) {
    setWasVisible(visible);
    if (visible) setIndex(0);
  }

  const dismiss = () => {
    if (!settings.onboardingSeen) update({ onboardingSeen: true });
    close();
  };

  const Body = step.Body;

  return (
    <Dialog
      visible={visible}
      title={t.onboarding.title}
      onClose={dismiss}
      className="onboarding_modal"
    >
      <DialogHeader
        title={t.onboarding.title}
        titleId={ONBOARDING_TITLE_ID}
        onClose={dismiss}
      />
      <div className="onboarding_body">
        <h3 className="onboarding_step_title">{step.title()}</h3>
        <p className="onboarding_step_description">{step.description()}</p>
        {Body && (
          <div className="onboarding_step_body">
            <Body settings={settings} update={update} />
          </div>
        )}
        <div className="onboarding_steps">
          <ol className="onboarding_progress" aria-hidden="true">
            {steps.map((s, i) => (
              <li
                key={s.id}
                className={classNames(
                  "onboarding_dot",
                  i === index && "active",
                  i < index && "done",
                )}
              />
            ))}
          </ol>
        </div>
        <DialogActions>
          {!last && (
            <Button className="onboarding_skip" onClick={dismiss}>
              {t.onboarding.skip}
            </Button>
          )}
          {index > 0 && (
            <Button onClick={() => setIndex((i) => Math.max(0, i - 1))}>
              {t.onboarding.back}
            </Button>
          )}
          {last ? (
            <Button className="onboarding_primary" onClick={dismiss}>
              {t.onboarding.finish}
            </Button>
          ) : (
            <Button
              className="onboarding_primary"
              onClick={() => setIndex((i) => i + 1)}
            >
              {t.onboarding.next}
            </Button>
          )}
        </DialogActions>
      </div>
    </Dialog>
  );
};

export default OnboardingDialog;
