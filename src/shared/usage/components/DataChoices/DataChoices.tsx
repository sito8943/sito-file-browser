import Button, { BUTTON_VARIANT } from "@/shared/components/elements/Button";
import { t } from "@/lang";

import "@/styles/components/DataChoices.css";

import { DATA_CHOICES_TITLE_ID } from "../../constants";
import { useUsage } from "../../providers/UsageContext";

// Asks once whether anonymous usage counts may be sent. Accept and decline carry the same weight;
// the choice can be changed later in Settings → General → Privacy. Non-modal, so it never blocks
// the browser or the welcome guide.
const DataChoices = () => {
  const { isNoticeOpen, setAnalytics } = useUsage();

  if (!isNoticeOpen) return null;

  return (
    <section
      className="DataChoices"
      role="dialog"
      aria-live="polite"
      aria-labelledby={DATA_CHOICES_TITLE_ID}
    >
      <h2 id={DATA_CHOICES_TITLE_ID} className="data_choices_title">
        {t.privacy.notice.title}
      </h2>
      <p className="data_choices_body">{t.privacy.notice.body}</p>
      <div className="data_choices_actions">
        <Button
          variant={BUTTON_VARIANT.OUTLINED}
          onClick={() => setAnalytics(false)}
        >
          {t.privacy.notice.decline}
        </Button>
        <Button
          variant={BUTTON_VARIANT.OUTLINED}
          onClick={() => setAnalytics(true)}
        >
          {t.privacy.notice.accept}
        </Button>
      </div>
    </section>
  );
};

export default DataChoices;
