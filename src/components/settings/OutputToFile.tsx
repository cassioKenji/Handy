import React from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface OutputToFileProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** The text file the transcription is written to, instead of being pasted. */
export const OutputFilePath: React.FC<OutputToFileProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const path = getSetting("output_file_path") ?? null;

    const handleChoose = async () => {
      const chosen = await save({
        title: t("settings.advanced.outputFile.dialogTitle"),
        defaultPath: path ?? "transcriptions.txt",
        filters: [
          {
            name: t("settings.advanced.outputFile.filter"),
            extensions: ["txt"],
          },
        ],
      });
      // Cancelled: keep what was there.
      if (chosen) {
        await updateSetting("output_file_path", chosen);
      }
    };

    return (
      <SettingContainer
        title={t("settings.advanced.outputFile.title")}
        description={t("settings.advanced.outputFile.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        layout="stacked"
      >
        <div className="flex items-center gap-2">
          <div className="flex-1 min-w-0 px-2 py-2 bg-wash border border-line-strong rounded-lg text-xs font-mono break-all select-text cursor-text">
            {path ?? t("settings.advanced.outputFile.none")}
          </div>
          <Button
            onClick={handleChoose}
            variant="secondary"
            size="sm"
            disabled={isUpdating("output_file_path")}
            className="px-3 py-2"
          >
            {path
              ? t("settings.advanced.outputFile.change")
              : t("settings.advanced.outputFile.choose")}
          </Button>
        </div>
      </SettingContainer>
    );
  },
);

/** Switch: write to the file instead of pasting. Needs a file first. */
export const OutputToFile: React.FC<OutputToFileProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const enabled = getSetting("output_to_file_enabled") ?? false;
    const hasFile = Boolean(getSetting("output_file_path"));

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(value) => updateSetting("output_to_file_enabled", value)}
        disabled={!hasFile && !enabled}
        isUpdating={isUpdating("output_to_file_enabled")}
        label={t("settings.advanced.outputToFile.label")}
        description={
          hasFile
            ? t("settings.advanced.outputToFile.description")
            : t("settings.advanced.outputToFile.needsFile")
        }
        descriptionMode={descriptionMode}
        grouped={grouped}
        tooltipPosition="bottom"
      />
    );
  },
);
