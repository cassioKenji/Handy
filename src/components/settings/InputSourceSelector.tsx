import React from "react";
import { useTranslation } from "react-i18next";
import type { InputSource } from "@/bindings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";

interface InputSourceSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** Microphone, or what the computer is playing (loopback). */
export const InputSourceSelector: React.FC<InputSourceSelectorProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const {
      getSetting,
      updateSetting,
      isUpdating,
      isLoading,
      loopbackSupported,
    } = useSettings();
    const source: InputSource = getSetting("input_source") ?? "microphone";

    const options = [
      {
        value: "microphone",
        label: t("settings.sound.inputSource.microphone"),
      },
      {
        value: "system_output",
        label: t("settings.sound.inputSource.systemOutput"),
      },
    ];

    return (
      <SettingContainer
        title={t("settings.sound.inputSource.title")}
        description={
          loopbackSupported
            ? t("settings.sound.inputSource.description")
            : t("settings.sound.inputSource.unsupported")
        }
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={options}
          selectedValue={source}
          onSelect={(value) =>
            updateSetting("input_source", value as InputSource)
          }
          disabled={
            isLoading ||
            isUpdating("input_source") ||
            // Still let someone on an unsupported OS switch back.
            (!loopbackSupported && source === "microphone")
          }
        />
      </SettingContainer>
    );
  });
