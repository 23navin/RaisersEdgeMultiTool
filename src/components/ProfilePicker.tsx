// components/ProfilePicker.tsx
//
// Renders the profile dropdown at the top of the sidebar.
// Calls onSelect when the user picks a profile.
// Does not call invoke() — App.tsx owns that.

import type { ProfileSummary } from "../types";

type Props = {
  profiles: ProfileSummary[];
  selected: ProfileSummary | null;
  onSelect: (profile: ProfileSummary) => void;
  loading: boolean;
};

export function ProfilePicker({ profiles, selected, onSelect, loading }: Props) {
  function handleChange(e: React.ChangeEvent<HTMLSelectElement>) {
    const profile = profiles.find((p) => p.zip_path === e.target.value);
    if (profile) onSelect(profile);
  }

  return (
    <div className="profile-picker">
      <label htmlFor="profile-select">Import Profile</label>
      <select
        id="profile-select"
        value={selected?.zip_path ?? ""}
        onChange={handleChange}
        disabled={loading || profiles.length === 0}
      >
        <option value="" disabled>
          {profiles.length === 0 ? "No profiles found" : "Select a profile..."}
        </option>
        {profiles.map((p) => (
          <option key={p.zip_path} value={p.zip_path}>
            {p.name} — v{p.version}
          </option>
        ))}
      </select>
      {loading && <span className="loading-hint">Loading...</span>}
    </div>
  );
}