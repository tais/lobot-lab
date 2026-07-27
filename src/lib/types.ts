export interface DataRoot {
  path: string;
  label: string;
}

export interface Character {
  id: number;
  name: string;
  nickname: string;
  bodyType: number;
  bodyTypeName: string;
  faceIndex: number;
  hairPalette: string;
  skinPalette: string;
  vestPalette: string;
  pantsPalette: string;
}

export interface Item {
  id: number;
  name: string;
  itemClass: number;
  compatibleSlots: string[];
  lbeClass?: number;
  lbeCombo?: number;
  camouflageKit: boolean;
}

export interface WorkspaceSummary {
  roots: DataRoot[];
  characters: Character[];
  items: Item[];
  layers: number;
  surfaces: number;
  filters: number;
  bodyTypes: number;
  warningCount: number;
  diagnostics: Diagnostic[];
}

export interface Diagnostic {
  severity: "info" | "warning" | "error";
  code: string;
  message: string;
  source?: string;
}

export interface Inventory {
  [slot: string]: number;
}

export interface Attachments {
  [slot: string]: number[];
}

export interface AttachmentOption {
  id: number;
  name: string;
}

export interface ScenarioState {
  team: number;
  soldierClass: number;
  civilianGroup: number;
  camo: number;
  urbanCamo: number;
  desertCamo: number;
  snowCamo: number;
  inWater: boolean;
  injured: boolean;
  bigMercAlt: boolean;
  bigMercBadass: boolean;
  secondHandUsable: boolean;
  secondHandLoaded: boolean;
  burst: boolean;
}

export interface PreviewRequest {
  characterId: number;
  inventory: Inventory;
  attachments: Attachments;
  scenario: ScenarioState;
  animation: string;
  direction: number;
  frame: number;
}

export interface Animation {
  id: string;
  label: string;
  group: string;
  resolvedSurface: string;
  variant: string;
  framesPerDirection: number;
  layerCount: number;
}

export interface PreviewContext {
  bodyType: string;
  weaponMode: string;
  profilePalette: {
    hair: string;
    skin: string;
    vest: string;
    pants: string;
  };
  camouflage: {
    appliedLimit: number;
    applied: [number, number, number, number];
    worn: [number, number, number, number];
    total: [number, number, number, number];
    stealth: number;
    palette: string;
  };
  animations: Animation[];
}

export interface PreviewLayer {
  layer: string;
  zIndex: number;
  surface?: string;
  file?: string;
  filter?: string;
  palette?: string;
  spriteDirection?: number;
  imageIndex?: number;
  status:
    | "rendered"
    | "hidden"
    | "missing-surface"
    | "missing-file"
    | "missing-frame"
    | "missing-alpha-file"
    | "missing-alpha-frame"
    | "unmatched";
  detail?: string;
}

export interface Preview {
  pngDataUrl?: string;
  width: number;
  height: number;
  bodyType: string;
  animationState: string;
  resolvedSurface: string;
  animationVariant: string;
  spriteDirection: number;
  imageIndex: number;
  layers: PreviewLayer[];
  diagnostics: Diagnostic[];
}

export interface AuditFinding {
  severity: "warning" | "error";
  code: string;
  animation: string;
  direction?: number;
  layer?: string;
  message: string;
}

export interface Audit {
  animationsChecked: number;
  surfacesChecked: number;
  issueCount: number;
  truncated: boolean;
  findings: AuditFinding[];
}
