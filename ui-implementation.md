# Import Tool — UI Implementation Prompt

> Hand this document to Claude Code to implement the React/Tauri frontend.
> The app is built with React + TypeScript + Tailwind + shadcn/ui inside a Tauri shell.
> All layout decisions, component specs, and state shapes are defined here.
> Do not deviate from the structure unless explicitly noted as flexible.

---

## Stack

- **Framework:** React + TypeScript (Vite)
- **Styling:** Tailwind CSS utility classes only — no custom CSS files
- **Components:** shadcn/ui (`Button`, `Select`, `Progress`)
- **Icons:** `@tauri-apps/api` for invoke; use `lucide-react` for icons
- **Desktop shell:** Tauri v2

Install shadcn components before starting:
```bash
npx shadcn@latest add button select progress
```

---

## File Structure

Create or modify these files only:

```
src/
├── App.tsx                        Root — holds all shared state, renders shell
├── components/
│   ├── Titlebar.tsx               Top bar: app name, tabs, settings, window controls
│   ├── Sidebar.tsx                Profile select + step checklist
│   ├── MainPanel.tsx              Scrollable content area — renders steps from profile
│   └── steps/
│       ├── StepSelectFiles.tsx    Step type: file_input
│       ├── StepGenerateFile.tsx   Step type: sql_transform
│       └── StepImport.tsx         Step type: manual_instruction
```

---

## Shared State (App.tsx)

Lift all shared state to `App.tsx` and pass down as props. Do not use a state management library.

```tsx
// Types
type FileStatus = 'none' | 'pending' | 'valid' | 'invalid'
type GenerateStatus = 'idle' | 'running' | 'done'

// State
const [selectedProfile, setSelectedProfile] = useState<string | null>(null)
const [profiles, setProfiles] = useState<ProfileMeta[]>([])           // from invoke("list_profiles")
const [fileName, setFileName] = useState<string | null>(null)         // selected file name
const [fileStatus, setFileStatus] = useState<FileStatus>('none')
const [generateStatus, setGenerateStatus] = useState<GenerateStatus>('idle')
const [generateProgress, setGenerateProgress] = useState<number>(0)   // 0–100
```

**Derived step completion** (compute from state, don't store separately):
```tsx
const stepsDone = {
  selectFiles: fileStatus === 'valid',
  generateFile: generateStatus === 'done',
  import: false, // manual step, never auto-completed
}
```

---

## Layout Shell

The app is a full-viewport flex column. No scrolling at the root level.

```
┌─────────────────────────────────────────────────────┐
│  Titlebar  (48px fixed height, transparent bg)      │
├─────────────────────────────────────────────────────┤
│  Workspace  (flex-1, p-4, bg-neutral-100)           │
│  ┌───────────────────────────────────────────────┐  │
│  │  Floating Panel  (flex row, h-full, rounded)  │  │
│  │  ┌──────────┬────────────────────────────┐   │  │
│  │  │ Sidebar  │  MainPanel (overflow-y-auto)│   │  │
│  │  │ 214px    │  flex-1                    │   │  │
│  │  └──────────┴────────────────────────────┘   │  │
│  └───────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

```tsx
// App.tsx skeleton
<div className="flex flex-col h-screen bg-neutral-100">
  <Titlebar />
  <div className="flex-1 p-4 pt-0 overflow-hidden">
    <div className="flex h-full bg-white rounded-xl border border-neutral-200 shadow-md overflow-hidden">
      <Sidebar ... />
      <MainPanel ... />
    </div>
  </div>
</div>
```

---

## Component: Titlebar

**File:** `src/components/Titlebar.tsx`

### Layout
Three sections in a horizontal flex row, `h-[48px]`, transparent background, no border.

```
[ App Name/Logo ]   [ Import | Data request | Reports ]   [ ⚙ — □ × ]
  left-aligned           center, pill tabs               right-aligned
```

### App Name (left)
- Icon: `DatabaseIcon` (lucide) at 17px, muted color
- Text: "App Name" — placeholder, 13px, font-weight 500
- Left padding: `pl-[10px]` to align with sidebar's inner text
- `min-w-[140px]`

### Tab Pill (center)
Three tabs inside a pill container. Style the **container** with a shaded fill, not individual tabs.

```tsx
// Container
<div className="flex items-center bg-neutral-200/70 rounded-[10px] p-[3px] gap-[1px]">
  <TabItem icon={FileImportIcon} label="Import" active />
  <TabItem icon={SendIcon}       label="Data request" />
  <TabItem icon={BarChartIcon}   label="Reports" />
</div>
```

Each tab item:
- Height: `h-[26px]`, padding: `px-[13px]`
- Border radius: `rounded-[8px]`
- Inactive: `text-neutral-400`, no background
- **Active:** `bg-white text-neutral-900 font-medium shadow-sm`
- Icon size: 14px, gap between icon and label: 6px
- "Data request" and "Reports" are visually present but non-functional (no `onClick`)

### Right Controls
Settings button + Windows window controls (minimize, maximize, close).
- Each button: `w-[26px] h-[26px]`, `rounded-[7px]`, no border, transparent background
- Hover: `hover:bg-black/5`
- Close hover: `hover:bg-red-100 hover:text-red-600`
- Settings button has 6px right margin before the window controls
- **macOS note:** Tauri provides `data-tauri-drag-region` for the drag area. Window controls should be conditionally rendered based on `platform()` from `@tauri-apps/plugin-os` — left side on macOS, right side on Windows. For now, implement Windows layout only.

---

## Component: Sidebar

**File:** `src/components/Sidebar.tsx`

**Width:** `w-[214px]` fixed, `border-r border-neutral-200`

### Profile Select Row
```
[ 🗂 icon ]  [ Select dropdown ▼ ]
```
- Row padding: `px-[10px] pb-[10px]`
- Icon: `FileImportIcon` at 16px, `text-neutral-400`
- Select: shadcn `<Select>` component, `h-[30px]`, fills remaining width
- Populated from `profiles` prop (array from `invoke("list_profiles")`)

### Step List
Sits directly below the select with **no label or separator** between them.

**Vertical rule positioning:**
- The rule's left edge must align with the **horizontal center of the icon** above it
- Icon center = `10px (row padding-left) + 8px (half of 16px icon width)` = `18px` from sidebar edge
- Implement as: `ml-[17px] pl-[14px] border-l-[1.5px] border-neutral-200 mr-[10px]`

Each step item:
```
[ step label text ]    [ ✓ green check if done ]
```
- `flex items-center justify-between`
- Padding: `px-[6px] py-[5px]`
- Border radius: `rounded-md`
- Hover: `hover:bg-neutral-50`
- Label: 13px, `text-neutral-500` when incomplete, `text-neutral-900` when done
- Check icon: `CheckIcon` (lucide) at 13px, `text-green-500`, only rendered when done
- Step order and labels come from the profile's `steps` array

---

## Component: MainPanel

**File:** `src/components/MainPanel.tsx`

`flex-1 overflow-y-auto px-[22px] py-[18px]`

All content scrolls together — no frozen header.

### Profile Header (scrolls with content)
```tsx
<div className="pb-[14px] border-b border-neutral-200 mb-[20px]">
  <h1 className="text-[17px] font-medium text-neutral-900">{profile.name}</h1>
  <p className="text-[13px] text-neutral-500 leading-relaxed mt-[3px]">{profile.description}</p>
</div>
```

### Step Rendering
Steps are rendered in order from the profile's `steps` array.
Each step `type` maps to a component:

| `type` | Component |
|---|---|
| `file_input` | `<StepSelectFiles>` |
| `sql_transform` | `<StepGenerateFile>` |
| `manual_instruction` | `<StepImport>` |

Each step section has a standard heading row:
```tsx
<div className="flex items-center justify-between mb-[7px]">
  <h2 className="text-[14px] font-medium text-neutral-900">{step.label}</h2>
  {isDone && <CheckIcon size={14} className="text-green-500" />}
</div>
```

Gap between step sections: `gap-[20px]` in a flex-col container.

---

## Component: StepSelectFiles

**File:** `src/components/steps/StepSelectFiles.tsx`

Rendered for steps with `type: file_input`.

### Structure
```
[step heading row]
[description text from MD]
[inner card]
  └── [Classification label] [Upload btn] [filename pill] [Validate btn]
```

### Description
```tsx
<p className="text-[13px] text-neutral-500 leading-relaxed mb-[7px]">
  {step.description}  {/* rendered from MD, supports inline <code> */}
</p>
```
Inline `<code>` style: `font-mono text-[12px] bg-neutral-100 px-[5px] py-[1px] rounded border border-neutral-200`

### Inner Card
```tsx
<div className="bg-neutral-100 rounded-xl px-[12px] py-[8px]">
  <div className="flex items-center gap-[8px]">
    ...
  </div>
</div>
```
No border on the card.

### File Row Elements (left to right)

**Input label**
- Text from `input.label` in YAML (e.g. "Classification")
- `text-[12px] font-medium text-neutral-900 whitespace-nowrap`

**Upload button**
- shadcn `<Button>` with custom classes to override default styles
- Style: `bg-[#e0ddd8] text-neutral-900 rounded-[10px] h-[32px] px-[14px] text-[13px] font-medium border-none hover:brightness-95`
- Icon: `UploadIcon` at 13px
- `onClick`: opens Tauri file dialog via `invoke("open_file_dialog")` or `open()` from `@tauri-apps/plugin-dialog`, filtered to `input.accepts` extensions
- On file selected: sets `fileName`, sets `fileStatus` to `'pending'`

**Filename pill**
- `flex-1 h-[28px] flex items-center px-[9px] rounded-md border font-mono text-[12px] overflow-hidden text-ellipsis whitespace-nowrap`
- Empty state: `bg-white border-neutral-200 text-neutral-400`, text: "no file selected"
- Has file: `bg-white border-neutral-300 text-neutral-900`
- Displays `fileName` prop

**Validate button**
- Same base button style as Upload: `rounded-[10px] h-[32px] px-[14px] text-[13px] font-medium border-none`
- Three visual states driven by `fileStatus`:

| `fileStatus` | Background | Text color | Icon |
|---|---|---|---|
| `none` or `pending` | `bg-[#e0ddd8]` | `text-neutral-900` | `ListChecksIcon` |
| `valid` | `bg-green-100` | `text-green-700` | `CheckIcon` |
| `invalid` | `bg-red-100` | `text-red-700` | `XIcon` |

- `onClick`: calls `invoke("validate_file", { filePath, profileName })` → sets `fileStatus` to `'valid'` or `'invalid'`
- Disabled when `fileStatus === 'none'`

---

## Component: StepGenerateFile

**File:** `src/components/steps/StepGenerateFile.tsx`

Rendered for steps with `type: sql_transform`.

### Structure
```
[step heading row]
[inner card]
  ├── [pipeline diagram]
  └── [Generate btn] [progress bar] [Download btn]
```

### Inner Card
Same card style as StepSelectFiles: `bg-neutral-100 rounded-xl px-[12px] py-[10px]`, no border.

### Pipeline Diagram
Full-width flex row. The diagram must span the entire card width with the DB icon centered.

```tsx
<div className="flex items-center w-full mb-[10px]">
  {/* Input nodes column — flex-1, left-aligned */}
  <div className="flex flex-col gap-[5px] flex-shrink-0">
    {inputs.map(input => <PipeNode key={input} icon={FileTextIcon} label={input} />)}
  </div>

  {/* Left arrow — flex-1, stretches */}
  <div className="flex-1 flex items-center min-w-[20px]">
    <ArrowSVG />  {/* see below */}
  </div>

  {/* DB icon — centered, fixed */}
  <div className="flex-shrink-0 px-[4px]">
    <DatabaseIcon size={22} className="text-neutral-400" />
  </div>

  {/* Right arrow — flex-1, stretches */}
  <div className="flex-1 flex items-center min-w-[20px]">
    <ArrowSVG />
  </div>

  {/* Output nodes column — flex-1, right-aligned */}
  <div className="flex flex-col gap-[5px] flex-shrink-0">
    {outputs.map(output => <PipeNode key={output} icon={TableIcon} label={output} />)}
  </div>
</div>
```

**ArrowSVG** — inline SVG that stretches to fill available width. Use `preserveAspectRatio="none"` so the line scales. The arrowhead polygon must sit at the end of the line exactly.

```tsx
function ArrowSVG() {
  return (
    <svg
      width="100%" height="12"
      viewBox="0 0 100 12"
      preserveAspectRatio="none"
      overflow="visible"
    >
      <line x1="0" y1="6" x2="91" y2="6" stroke="#d1d5db" strokeWidth="1.2" />
      <polygon points="91,2 100,6 91,10" fill="#d1d5db" />
    </svg>
  )
}
```

**PipeNode**
```tsx
<div className="inline-flex items-center gap-[5px] px-[10px] py-[4px] rounded-md bg-white border border-neutral-200 text-neutral-500 text-[11px] whitespace-nowrap shadow-sm">
  <Icon size={12} />
  {label}
</div>
```

Inputs come from `step.input[]`, outputs from `step.output[]` in the YAML.
The SQL file name is **not shown** — the DB icon represents the transformation generically.

### Generate Row

```
[Generate btn]  [progress bar, flex-1]  [Download btn]
```

**Generate button**
- Style: `bg-neutral-900 text-white rounded-[10px] h-[32px] px-[14px] text-[13px] font-medium border-none hover:brightness-110`
- Icon: `PlayIcon` at 13px
- `onClick`: calls `invoke("run_profile", { filePath, profileName, outputDir })`, drives progress updates
- Disabled when `fileStatus !== 'valid'`

**Progress bar**
- `flex-1 h-[5px] rounded-full bg-white overflow-hidden`
- Fill: `bg-neutral-400` during running, `bg-green-500` when `generateStatus === 'done'`
- Use shadcn `<Progress>` or a plain div with inline width style

**Download button**
- Same style as Upload button: `bg-[#e0ddd8] text-neutral-900 rounded-[10px] h-[32px] px-[14px] text-[13px] font-medium border-none hover:brightness-95`
- Icon: `DownloadIcon` at 13px
- `onClick`: opens save dialog or triggers file download — no text change on click
- Disabled when `generateStatus !== 'done'`

---

## Component: StepImport

**File:** `src/components/steps/StepImport.tsx`

Rendered for steps with `type: manual_instruction`.

This step renders the profile's MD content in **markdown document order** with no card wrapper around anything. Everything is flat prose.

### MD rendering order for the Simple Import profile:

```
[step heading row]                         ← standard heading with green check when done
[paragraph: "Import into the database..."] ← text before the image tag in MD
[image block]                              ← rendered from ![alt](path) in MD
[paragraph: "If there are exceptions..."]  ← text after the image tag in MD
```

### Paragraph style
```tsx
<p className="text-[13px] text-neutral-500 leading-relaxed">
  {/* inline <code> same style as StepSelectFiles */}
</p>
```

### Image block
The image path from the MD (`assets/import_profile.png`) is resolved relative to the loaded profile bundle's extracted assets folder.

**While the image loads or if it fails**, render a placeholder:
```tsx
<div className="h-[80px] rounded-md border border-neutral-200 bg-neutral-50 flex items-center justify-content gap-[8px] text-neutral-400 text-[12px] my-[10px]">
  <ImageIcon size={18} />
  {altText}
</div>
```

When the image resolves, render as a standard `<img>` tag with `rounded-md` and `w-full`.

Gap between paragraph and image: `my-[10px]` on the image block.

---

## invoke() Calls Summary

| Call | Where | Trigger | Returns |
|---|---|---|---|
| `list_profiles` | `App.tsx` | on mount | `ProfileMeta[]` |
| `open_file_dialog` | `StepSelectFiles` | Upload click | `string` (file path) |
| `validate_file` | `StepSelectFiles` | Validate click | `{ valid: boolean, errors: string[] }` |
| `run_profile` | `StepGenerateFile` | Generate click | `{ outputPath: string, rowCount: number }` |

All `invoke()` calls must be wrapped in `try/catch`. On error, surface the message inline near the triggering element — do not use alerts or console.log only.

---

## Colors (Hardcoded — do not use Tailwind color variables for these)

| Element | Value |
|---|---|
| Default button bg | `#e0ddd8` |
| Primary button bg | `#1a1a1a` |
| Primary button text | `#ffffff` |
| Valid button bg | `#bbf7d0` |
| Valid button text | `#15803d` |
| Invalid button bg | `#fecaca` |
| Invalid button text | `#b91c1c` |
| Green check icons | `text-green-500` (Tailwind OK here) |
| Progress fill (done) | `bg-green-500` |
| Arrow / pipe stroke | `#d1d5db` |

---

## Implementation Order

Build and verify each item before moving to the next. Do not wire `invoke()` until the UI renders correctly with mock data.

1. `App.tsx` — shell layout only, hardcoded `selectedProfile = "simple"`, no invoke yet
2. `Titlebar.tsx` — static, all three tabs visible, Import active
3. `Sidebar.tsx` — hardcoded step list, checkmark on step 1 only
4. `MainPanel.tsx` — renders all three step sections with placeholder content
5. `StepSelectFiles.tsx` — full file row UI with mock `fileStatus = 'valid'`
6. `StepGenerateFile.tsx` — pipeline diagram + generate row, mock `generateStatus = 'done'`
7. `StepImport.tsx` — flat markdown rendering with image placeholder
8. Wire `invoke("list_profiles")` → populate sidebar select
9. Wire `invoke("open_file_dialog")` + `invoke("validate_file")` → file row states
10. Wire `invoke("run_profile")` → progress bar + generate states

---

## Notes for Claude Code

- Do not create custom CSS files. Use Tailwind utility classes exclusively.
- Do not install additional UI libraries beyond shadcn.
- Do not use `React.useEffect` for anything other than data fetching on mount.
- The profile's `steps` array in the YAML drives rendering order — do not hardcode step order in the components.
- `StepGenerateFile` must handle multiple inputs and multiple outputs gracefully (the pipeline diagram stacks nodes vertically in each column).
- All button `border` properties must be explicitly set to `none` or `border-0` — shadcn Button defaults include a border that must be overridden.
