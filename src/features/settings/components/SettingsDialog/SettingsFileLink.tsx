import { useEffect, useState } from "react";

import Button from "@/shared/components/elements/Button";
import type { SettingsFileLocation } from "@/shared/services/api";
import { t } from "@/lang";

import { useSettings } from "../../providers/SettingsProvider";

// The live settings.toml path under the search box, so hand edits (or keys without a control yet)
// are one click away. Clicking opens its folder in a new file browser window, like the Storage
// panel's paths. Renders nothing until the path resolves.
const SettingsFileLink = () => {
  const { manager } = useSettings();
  const [location, setLocation] = useState<SettingsFileLocation | null>(null);

  useEffect(() => {
    let active = true;
    manager
      .getSettingsFile()
      .then((result) => {
        if (active) setLocation(result);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [manager]);

  if (!location) return null;

  return (
    <div className="settings_file">
      <Button
        unstyled
        className="settings_storage_path text_link"
        title={t.settings.settingsFileOpen}
        onClick={() => void manager.openPath(location.dir)}
      >
        {location.file}
      </Button>
    </div>
  );
};

export default SettingsFileLink;
