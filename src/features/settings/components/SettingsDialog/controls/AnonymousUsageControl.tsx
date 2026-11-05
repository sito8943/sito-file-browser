import Switcher from "@/shared/components/elements/Switcher";
import { useUsage } from "@/shared/usage";

// Anonymous usage counts on/off. The choice is the consent stored by the usage provider (same as
// the first-run notice), not an AppSettings field, so it ignores CustomControlProps.
const AnonymousUsageControl = () => {
  const { analytics, setAnalytics } = useUsage();
  return (
    <Switcher
      checked={analytics === true}
      onChange={() => setAnalytics(analytics !== true)}
    />
  );
};

export default AnonymousUsageControl;
