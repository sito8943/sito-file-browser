import { useFolderPicker } from "@/shared/providers/FolderPickerProvider";
import { useRemote } from "@/features/remote";
import { t } from "@/lang";

import SettingsButton from "../SettingsButton";

// "Share a folder": pick a folder and authorize it. The backend refuses unsafe choices (whole
// disks, home, system and network folders) with a code shown as a toast.
const RemoteRootsControl = () => {
  const { addRoot } = useRemote();
  const { pickFolder } = useFolderPicker();
  const onClick = async () => {
    const path = await pickFolder();
    if (path) await addRoot(path);
  };
  return (
    <SettingsButton onClick={() => void onClick()}>
      {t.remoteControl.rootAdd}
    </SettingsButton>
  );
};

export default RemoteRootsControl;
