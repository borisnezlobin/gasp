import { App, PluginSettingTab, Setting } from "obsidian";
import type FootnotesPlusPlugin from "./main";

export interface FootnotesPlusSettings {
  autoRenumberOnEdit: boolean;
  jumpToNewDefinition: boolean;
}

export const DEFAULT_SETTINGS: FootnotesPlusSettings = {
  autoRenumberOnEdit: true,
  jumpToNewDefinition: true,
};

export class FootnotesPlusSettingTab extends PluginSettingTab {
  plugin: FootnotesPlusPlugin;

  constructor(app: App, plugin: FootnotesPlusPlugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display(): void {
    const { containerEl } = this;
    containerEl.empty();

    new Setting(containerEl)
      .setName("Keyboard shortcut")
      .setDesc(
        'Open Obsidian Settings → Hotkeys, search for "Insert or jump to footnote", then select the plus button to assign or change its shortcut.'
      );

    new Setting(containerEl)
      .setName("Auto-renumber while editing")
      .setDesc(
        "Keep footnotes in ascending order automatically after you insert or delete one. Turn this off to renumber only with the Tidy command."
      )
      .addToggle((toggle) =>
        toggle.setValue(this.plugin.settings.autoRenumberOnEdit).onChange(async (value) => {
          this.plugin.settings.autoRenumberOnEdit = value;
          await this.plugin.saveSettings();
        })
      );

    new Setting(containerEl)
      .setName("Jump to the new footnote after inserting")
      .setDesc(
        "After inserting a footnote, move the cursor to its definition at the bottom so you can type it right away."
      )
      .addToggle((toggle) =>
        toggle.setValue(this.plugin.settings.jumpToNewDefinition).onChange(async (value) => {
          this.plugin.settings.jumpToNewDefinition = value;
          await this.plugin.saveSettings();
        })
      );
  }
}
