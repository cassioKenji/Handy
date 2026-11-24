import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";

interface SystemOutputSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** The outputs that can be recorded, as a dropdown. */
const SystemOutputDropdown: React.FC = () => {
  const { t } = useTranslation();
  const {
    getSetting,
    updateSetting,
    isUpdating,
    isLoading,
    loopbackDevices,
    refreshLoopbackDevices,
  } = useSettings();

  const selectedOutput =
    getSetting("selected_loopback_device") === "default"
      ? "Default"
      : getSetting("selected_loopback_device") || "Default";

  const handleOutputSelect = async (deviceName: string) => {
    await updateSetting("selected_loopback_device", deviceName);
  };

  const outputOptions = loopbackDevices.map((device) => ({
    value: device.name,
    label: device.name,
  }));

  return (
    <Dropdown
      options={outputOptions}
      selectedValue={selectedOutput}
      onSelect={handleOutputSelect}
      placeholder={
        isLoading || loopbackDevices.length === 0
          ? t("settings.sound.systemOutput.loading")
          : t("settings.sound.systemOutput.placeholder")
      }
      disabled={
        isUpdating("selected_loopback_device") ||
        isLoading ||
        loopbackDevices.length === 0
      }
      onOpen={refreshLoopbackDevices}
    />
  );
};

export const SystemOutputSelector: React.FC<SystemOutputSelectorProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { resetSetting, isUpdating, isLoading } = useSettings();

    const handleReset = async () => {
      await resetSetting("selected_loopback_device");
    };

    return (
      <SettingContainer
        title={t("settings.sound.systemOutput.title")}
        description={t("settings.sound.systemOutput.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex items-center space-x-1">
          <SystemOutputDropdown />
          <ResetButton
            ariaLabel={t("common.resetSetting", {
              setting: t("settings.sound.systemOutput.title"),
            })}
            onClick={handleReset}
            disabled={isUpdating("selected_loopback_device") || isLoading}
          />
        </div>
      </SettingContainer>
    );
  });
