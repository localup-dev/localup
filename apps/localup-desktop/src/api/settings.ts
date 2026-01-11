import { invoke } from "@tauri-apps/api/core";

export interface AppSettings {
  autostart: boolean;
  start_minimized: boolean;
  auto_connect_tunnels: boolean;
  capture_traffic: boolean;
  clear_on_close: boolean;
}

export type SettingKey =
  | "autostart"
  | "start_minimized"
  | "auto_connect_tunnels"
  | "capture_traffic"
  | "clear_on_close";

/**
 * Get all application settings
 */
export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

/**
 * Update a single setting
 */
export async function updateSetting(
  key: SettingKey,
  value: boolean
): Promise<void> {
  return invoke("update_setting", { key, value });
}

/**
 * Get the current autostart status from the system
 */
export async function getAutostartStatus(): Promise<boolean> {
  return invoke<boolean>("get_autostart_status");
}
