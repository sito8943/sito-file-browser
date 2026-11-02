import { useEffect, useState } from "react";

import Dialog from "@/shared/components/patterns/Dialog";
import DialogHeader from "@/shared/components/patterns/DialogHeader";
import DialogActions from "@/shared/components/patterns/DialogActions";
import Button, { BUTTON_VARIANT } from "@/shared/components/elements/Button";
import type { RemotePairingCode } from "@/shared/services/api";
import { notify, TOAST_TYPE } from "@/shared/toast";
import { t } from "@/lang";

import "@/styles/components/PairDeviceDialog.css";

import { useRemote } from "../../providers/RemoteProvider";
import { PAIR_DEVICE_TITLE_ID, PAIRING_TTL_MS } from "../../constants";
import { formatSas, remoteErrorMessage, svgDataUrl } from "../../utils";

// Pairing (security rule 4): shows a one-time QR for 2 minutes; once the phone proves the code,
// switches to the 6-digit SAS that both screens must show, and only "Codes match" trusts the
// phone. Closing at any point cancels the pairing window on the desktop.
const PairDeviceDialog = () => {
  const { manager, pairingVisible, pending, closePairing, confirmPairing } =
    useRemote();
  const [code, setCode] = useState<RemotePairingCode | null>(null);
  const [expired, setExpired] = useState(false);
  const [generation, setGeneration] = useState(0);
  const [confirming, setConfirming] = useState(false);

  // Every open starts from a clean slate (no stale QR from the previous pairing).
  const [wasVisible, setWasVisible] = useState(pairingVisible);
  if (pairingVisible !== wasVisible) {
    setWasVisible(pairingVisible);
    if (pairingVisible) {
      setCode(null);
      setExpired(false);
      setConfirming(false);
    }
  }

  // Request a fresh code on open and on "New code"; mark it expired when its 2 minutes pass so the
  // user is not left scanning a dead QR.
  useEffect(() => {
    if (!pairingVisible) return;
    let active = true;
    manager
      .startPairing()
      .then((next) => {
        if (active) setCode(next);
      })
      .catch((error) => {
        if (!active) return;
        notify(remoteErrorMessage(error), TOAST_TYPE.ERROR);
        closePairing();
      });
    const timer = window.setTimeout(() => {
      if (active) setExpired(true);
    }, PAIRING_TTL_MS);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [pairingVisible, generation, manager, closePairing]);

  const renew = () => {
    setCode(null);
    setExpired(false);
    setGeneration((value) => value + 1);
  };

  const confirm = async () => {
    setConfirming(true);
    await confirmPairing();
    setConfirming(false);
  };

  return (
    <Dialog
      visible={pairingVisible}
      title={t.remoteControl.pairTitle}
      onClose={closePairing}
      className="pair_device_modal"
    >
      <DialogHeader
        title={t.remoteControl.pairTitle}
        titleId={PAIR_DEVICE_TITLE_ID}
        onClose={closePairing}
      />
      <div className="pair_device_body">
        {pending ? (
          <>
            <p className="pair_device_text">
              {t.remoteControl.pairCompare(pending.name)}
            </p>
            <p className="pair_device_sas">{formatSas(pending.sas)}</p>
          </>
        ) : (
          <>
            <p className="pair_device_text">{t.remoteControl.pairScan}</p>
            <div className="pair_device_qr">
              {code && !expired && (
                <img
                  className="pair_device_qr_image"
                  src={svgDataUrl(code.svg)}
                  alt={t.remoteControl.pairTitle}
                />
              )}
              {!code && !expired && (
                <span className="pair_device_hint">
                  {t.remoteControl.pairLoading}
                </span>
              )}
              {expired && (
                <span className="pair_device_hint">
                  {t.remoteControl.pairExpired}
                </span>
              )}
            </div>
            <p className="pair_device_hint">{t.remoteControl.pairExpiry}</p>
          </>
        )}
        <DialogActions>
          {pending ? (
            <>
              <Button onClick={closePairing} disabled={confirming}>
                {t.remoteControl.pairReject}
              </Button>
              <Button
                variant={BUTTON_VARIANT.PRIMARY}
                onClick={() => void confirm()}
                disabled={confirming}
              >
                {t.remoteControl.pairConfirm}
              </Button>
            </>
          ) : (
            <>
              {expired && (
                <Button onClick={renew}>{t.remoteControl.pairNewCode}</Button>
              )}
              <Button onClick={closePairing}>
                {t.remoteControl.pairCancel}
              </Button>
            </>
          )}
        </DialogActions>
      </div>
    </Dialog>
  );
};

export default PairDeviceDialog;
